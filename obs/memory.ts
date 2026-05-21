import os from "node:os";
import cp from "node:child_process";
import { promisify } from "node:util";
import prettyBytes from "pretty-bytes";

const execAsync = promisify(cp.exec);

export interface MemoryInfo {
  ramPctAvailable: number;
  ramFree: number;
  ramTotal: number;
  vramPctAvailable: number;
  vramFree: number;
  vramTotal: number;
}

export function memInfoToString({ ramFree, ramTotal, vramFree, vramTotal }: MemoryInfo): string {
  const ramInfo = `RAM: ${prettyBytes(ramFree)} / ${prettyBytes(ramTotal)}`;
  const vramInfo = vramTotal > 0 ? `VRAM: ${prettyBytes(vramFree)} / ${prettyBytes(vramTotal)}` : null;

  return vramInfo ? `${ramInfo} | ${vramInfo}` : ramInfo;
}

export async function getMemory(): Promise<MemoryInfo> {
  const ramTotal = os.totalmem();
  const ramFree = os.freemem();

  // Use nvidia-smi for VRAM if available (values in MiB, convert to bytes)
  let vramFree = 0;
  let vramTotal = 0;
  if (os.platform() === "linux") {
    try {
      const { stdout } = await execAsync(
        "nvidia-smi --query-gpu=memory.free,memory.total --format=csv,noheader,nounits",
        { encoding: "utf8" },
      );
      const [freeStr, totalStr] = stdout
        .trim()
        .split(",")
        .map((s) => s.trim());
      const free = parseInt(freeStr);
      const total = parseInt(totalStr);
      if (!isNaN(free) && !isNaN(total)) {
        vramFree = free * 1024 * 1024;
        vramTotal = total * 1024 * 1024;
      }
    } catch {
      // nvidia-smi not available or failed
    }
  }

  const ramPctAvailable = ramFree / ramTotal;
  const vramPctAvailable = vramTotal > 0 ? vramFree / vramTotal : 0;

  return { ramFree, ramTotal, vramFree, vramTotal, ramPctAvailable, vramPctAvailable };
}
