#!/usr/bin/env node
import { spawn } from "node:child_process";
import * as fs from "node:fs/promises";

const [, , reportName, command, ...args] = process.argv;

if (!reportName || !command) {
  console.error("usage: github-rust-test-summary.mjs <report-name> <command> [args...]");
  process.exit(2);
}

const child = spawn(command, args, {
  stdio: ["inherit", "pipe", "pipe"],
});

let output = "";

child.stdout.on("data", (chunk) => {
  process.stdout.write(chunk);
  output += chunk.toString("utf8");
});

child.stderr.on("data", (chunk) => {
  process.stderr.write(chunk);
  output += chunk.toString("utf8");
});

const exitCode = await new Promise((resolve) => {
  child.on("error", (error) => {
    console.error(error);
    resolve(1);
  });
  child.on("close", (code, signal) => {
    if (signal) {
      console.error(`${command} exited after signal ${signal}`);
      resolve(1);
    } else {
      resolve(code ?? 1);
    }
  });
});

await appendGithubSummary(reportName, output, exitCode);
process.exit(exitCode);

async function appendGithubSummary(name, text, exitCode) {
  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  if (!summaryPath) {
    return;
  }

  const plainText = stripAnsi(text);
  const summaries = parseRustTestSummaries(plainText);
  const failedTests = parseFailedTests(plainText);
  const totals = summaries.reduce(
    (acc, summary) => {
      acc.passed += summary.passed;
      acc.failed += summary.failed;
      acc.ignored += summary.ignored;
      acc.filtered += summary.filtered;
      acc.suites += 1;
      return acc;
    },
    { passed: 0, failed: 0, ignored: 0, filtered: 0, suites: 0 },
  );
  const totalTests = totals.passed + totals.failed + totals.ignored;
  const status = exitCode === 0 ? "Passed" : "Failed";

  const lines = [
    `## ${escapeMarkdown(name)}`,
    "",
    "| Metric | Result |",
    "| --- | ---: |",
    `| Status | ${status} |`,
    `| Test suites | ${totals.suites} |`,
    `| Tests | ${totals.passed} passed / ${totals.failed} failed / ${totals.ignored} ignored / ${totalTests} total |`,
    `| Filtered out | ${totals.filtered} |`,
  ];

  if (summaries.length === 0) {
    lines.push("", "No Rust test result lines were found. The command may have failed before running tests.");
  }

  if (failedTests.length > 0) {
    lines.push("", "<details>", "<summary>Failed tests</summary>", "", "| Test |", "| --- |");
    for (const testName of failedTests.slice(0, 50)) {
      lines.push(`| \`${escapeTableCell(testName)}\` |`);
    }
    if (failedTests.length > 50) {
      lines.push(`| ...and ${failedTests.length - 50} more |`);
    }
    lines.push("", "</details>");
  }

  lines.push("");
  await fs.appendFile(summaryPath, `${lines.join("\n")}\n`, "utf8");
}

function parseRustTestSummaries(text) {
  const summaries = [];
  const regex =
    /test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;/g;
  for (const match of text.matchAll(regex)) {
    summaries.push({
      passed: Number.parseInt(match[1], 10),
      failed: Number.parseInt(match[2], 10),
      ignored: Number.parseInt(match[3], 10),
      measured: Number.parseInt(match[4], 10),
      filtered: Number.parseInt(match[5], 10),
    });
  }
  return summaries;
}

function parseFailedTests(text) {
  const failures = new Set();
  const regex = /^test (.+) \.\.\. FAILED$/gm;
  for (const match of text.matchAll(regex)) {
    failures.add(match[1]);
  }
  return [...failures].sort();
}

function escapeMarkdown(value) {
  return String(value).replace(/[\\`*_{}[\]()#+\-.!|]/g, "\\$&");
}

function escapeTableCell(value) {
  return String(value).replace(/\|/g, "\\|").replace(/`/g, "\\`");
}

function stripAnsi(value) {
  return String(value).replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "");
}
