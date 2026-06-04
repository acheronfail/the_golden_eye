#include "cv_wrapper.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

const char *ge_cv_version(void) { return CV_VERSION; }

namespace {

// Lightweight phase timer. When the GE_CV_TIMING environment variable is set,
// each lap() logs the milliseconds elapsed since the previous lap to stderr;
// otherwise it costs a clock read and nothing more. Used to find where time
// goes in ge_cv_match_level().
struct PhaseTimer {
	using clock = std::chrono::steady_clock;
	clock::time_point start = clock::now();
	clock::time_point last = start;
	bool enabled = std::getenv("GE_CV_TIMING") != nullptr;

	void lap(const char *label) {
		clock::time_point now = clock::now();
		if (enabled) {
			double ms = std::chrono::duration<double, std::milli>(now - last).count();
			double total = std::chrono::duration<double, std::milli>(now - start).count();
			std::fprintf(stderr, "[ge_cv timing] %-22s %8.2f ms  (total %8.2f ms)\n", label, ms, total);
		}
		last = now;
	}
};

// Correlation needed to accept a mission/part/difficulty label match.
constexpr double kLabelThreshold = 0.70;
// A mission match this strong means the current scale is essentially exact, so
// the remaining (resolution-recovery) scales cannot improve on it and the scale
// search can stop early.
constexpr double kStrongLabel = 0.90;

// Fraction of the frame searched for the mission/part/difficulty labels. They
// always sit in the upper-left of the stats overlay, so only the top 50% /
// left 60% needs to be searched.
constexpr double kLabelRegionW = 0.60;
constexpr double kLabelRegionH = 0.50;

// Region searched for the time colons: they sit in the bottom 50% of the frame
// and the middle 50% horizontally. Anchoring the (full-frame) colon search to
// this box also discards label colons ("Time:", "Accuracy:") for free.
constexpr double kColonRegionX = 0.25;
constexpr double kColonRegionW = 0.50;
constexpr double kColonRegionY = 0.50;
constexpr double kColonRegionH = 0.50;
// Correlation needed to accept an individual digit/colon glyph.
constexpr double kGlyphThreshold = 0.78;

// Candidate scales applied to the templates when locating the stats overlay.
// The templates are authored at the user's native capture resolution, so 1.0 is
// tried first and is the common case; the remaining scales let matching survive
// when the source is captured at a different resolution. A single global scale
// (the one that best fits the mission label) is then reused for every other
// template so the glyphs stay crisply aligned.
constexpr double kScales[] = {1.0, 0.9, 1.1, 0.8, 1.2, 0.75, 1.33, 0.67, 1.5, 0.6};

struct Detection {
	int x;        // left edge in the frame
	int y;        // top edge in the frame
	int w;        // glyph width at the matched scale
	double score; // correlation score
	int value;    // digit value 0-9 (unused for the colon)
};

// Loads "<dir>/<lang>-<name>.png" as a single-channel (grayscale) template.
// Returns an empty Mat when the file is missing or unreadable.
cv::Mat load_template(const std::string &dir, const char *lang, const std::string &name) {
	std::string path = dir + "/" + lang + "-" + name + ".png";
	return cv::imread(path, cv::IMREAD_GRAYSCALE);
}

// Returns `tmpl` resized by `scale` (or the original when scale == 1.0).
cv::Mat scaled(const cv::Mat &tmpl, double scale) {
	if (scale == 1.0) {
		return tmpl;
	}
	cv::Mat out;
	int w = std::max(1, (int)std::lround(tmpl.cols * scale));
	int h = std::max(1, (int)std::lround(tmpl.rows * scale));
	cv::resize(tmpl, out, cv::Size(w, h), 0, 0, scale < 1.0 ? cv::INTER_AREA : cv::INTER_LINEAR);
	return out;
}

// Best single-location match of `tmpl` against `frame`. Returns the peak
// correlation, or -1.0 if the template does not fit inside the frame.
double best_score(const cv::Mat &frame, const cv::Mat &tmpl) {
	if (tmpl.empty() || tmpl.rows > frame.rows || tmpl.cols > frame.cols) {
		return -1.0;
	}
	cv::Mat result;
	cv::matchTemplate(frame, tmpl, result, cv::TM_CCOEFF_NORMED);
	double maxVal = 0.0;
	cv::minMaxLoc(result, nullptr, &maxVal, nullptr, nullptr);
	return maxVal;
}

// Scores every template in `tmpls` against `frame` at `scale`, one template per
// parallel task. matchTemplate parallelises poorly on inputs this small, so the
// real speedup comes from spreading the independent calls across cores rather
// than threading within each call; calling it from inside parallel_for_ also
// makes its own (nested) parallelism run serially, avoiding oversubscription.
// Each task writes only its own slot, so no synchronisation is needed.
std::vector<double> best_scores(const cv::Mat &frame, const std::vector<cv::Mat> &tmpls, double scale) {
	std::vector<double> scores(tmpls.size(), -1.0);
	cv::parallel_for_(cv::Range(0, (int)tmpls.size()), [&](const cv::Range &r) {
		for (int i = r.start; i < r.end; ++i) {
			scores[i] = best_score(frame, scaled(tmpls[i], scale));
		}
	});
	return scores;
}

// Picks the highest-scoring template from `cv_templates` (matched at `scale`).
// Returns the 1-based index of the winner, or -1 when none clears the
// threshold. The templates are 0-indexed, so the returned value is index + 1.
int best_label(const cv::Mat &frame, const std::vector<cv::Mat> &templates, double scale, double threshold) {
	std::vector<double> scores = best_scores(frame, templates, scale);
	int best = -1;
	double bestScore = threshold;
	for (size_t i = 0; i < scores.size(); ++i) {
		if (scores[i] >= bestScore) {
			bestScore = scores[i];
			best = (int)i + 1;
		}
	}
	return best;
}

// Collects every location where `tmpl` matches `frame` above `threshold`.
void collect_detections(const cv::Mat &frame, const cv::Mat &tmpl, double threshold, int value,
                        std::vector<Detection> &out) {
	if (tmpl.empty() || tmpl.rows > frame.rows || tmpl.cols > frame.cols) {
		return;
	}
	cv::Mat result;
	cv::matchTemplate(frame, tmpl, result, cv::TM_CCOEFF_NORMED);
	for (int y = 0; y < result.rows; ++y) {
		const float *row = result.ptr<float>(y);
		for (int x = 0; x < result.cols; ++x) {
			if (row[x] >= threshold) {
				out.push_back({x, y, tmpl.cols, (double)row[x], value});
			}
		}
	}
}

// Greedy non-maximum suppression: keeps the strongest detection in each
// neighbourhood, dropping weaker ones whose centre lies within
// (cellW * frac, cellH * frac) of an already-kept detection.
std::vector<Detection> suppress(std::vector<Detection> dets, int cellW, int cellH, double frac) {
	std::sort(dets.begin(), dets.end(), [](const Detection &a, const Detection &b) { return a.score > b.score; });
	std::vector<Detection> kept;
	for (const Detection &d : dets) {
		bool overlaps = false;
		for (const Detection &k : kept) {
			if (std::abs(d.x - k.x) < cellW * frac && std::abs(d.y - k.y) < cellH * frac) {
				overlaps = true;
				break;
			}
		}
		if (!overlaps) {
			kept.push_back(d);
		}
	}
	return kept;
}

// A time recovered from the screen, kept with its position so the final array
// can be ordered top-to-bottom then left-to-right.
struct FoundTime {
	int y;
	int x;
	int seconds;
};

struct FoundMission {
	int mission;
	double score;
};

// Finds a mission number (1-9) by anchoring on ':' in the label region and
// taking the strongest single digit immediately to its left on the same line.
FoundMission find_mission_from_colons(const cv::Mat &labelRegion, const cv::Mat &colonTmpl,
	                                  const std::vector<cv::Mat> &digitTmpls) {
	if (colonTmpl.empty() || digitTmpls.size() < 10) {
		return {-1, -1.0};
	}

	int digitWidthSum = 0;
	for (int v = 1; v <= 9; ++v) {
		if (digitTmpls[v].empty()) {
			return {-1, -1.0};
		}
		digitWidthSum += digitTmpls[v].cols;
	}
	const int digitW = std::max(1, digitWidthSum / 9);
	const int digitH = digitTmpls[1].rows;
	const int colonW = colonTmpl.cols;
	const int colonH = colonTmpl.rows;

	std::vector<Detection> colons;
	collect_detections(labelRegion, colonTmpl, kGlyphThreshold, 0, colons);
	colons = suppress(std::move(colons), colonW, colonH, 0.5);

	FoundMission best = {-1, -1.0};
	const int bandPadX = digitW * 2;
	const int bandPadY = digitH;

	for (const Detection &colon : colons) {
		const int x0 = std::max(0, colon.x - bandPadX);
		const int y0 = std::max(0, colon.y - bandPadY);
		const int x1 = std::min(labelRegion.cols, colon.x + std::max(1, colonW / 2));
		const int y1 = std::min(labelRegion.rows, colon.y + colonH + bandPadY);
		if (x1 <= x0 || y1 <= y0) {
			continue;
		}

		const cv::Mat roi = labelRegion(cv::Rect(x0, y0, x1 - x0, y1 - y0));
		std::vector<std::vector<Detection>> perValue(10);
		cv::parallel_for_(cv::Range(1, 10), [&](const cv::Range &r) {
			for (int v = r.start; v < r.end; ++v) {
				collect_detections(roi, digitTmpls[v], kGlyphThreshold, v, perValue[v]);
			}
		});

		const double colonCenterY = colon.y + colonH / 2.0;
		for (int v = 1; v <= 9; ++v) {
			for (Detection d : perValue[v]) {
				d.x += x0;
				d.y += y0;
				if (std::abs((d.y + digitH / 2.0) - colonCenterY) >= digitH * 0.35) {
					continue;
				}
				if (d.x + d.w > colon.x + colonW * 0.7) {
					continue;
				}
				const double adjLeft = colon.x - (d.x + d.w);
				if (adjLeft < -digitW * 0.4 || adjLeft > digitW * 0.6) {
					continue;
				}
				if (d.score >= best.score) {
					best = {v, d.score};
				}
			}
		}
	}

	return best;
}

} // namespace

ge_level_match_result_t ge_cv_match_level(const uint8_t *bgra, uint32_t width, uint32_t height, const char *lang,
                                          const char *templates_dir) {
	ge_level_match_result_t result = {-1, -1, -1, nullptr, 0};

	if (!bgra || width == 0 || height == 0 || !lang || !templates_dir) {
		return result;
	}

	const std::string dir(templates_dir);

	// Load the label templates.
	std::vector<cv::Mat> parts, diffs;
	for (int i = 1; i <= 5; ++i) {
		parts.push_back(load_template(dir, lang, "part" + std::to_string(i)));
	}
	for (int i = 1; i <= 4; ++i) {
		diffs.push_back(load_template(dir, lang, "diff" + std::to_string(i)));
	}

	// Load base glyph templates once; mission and time matching both scale from
	// these in-memory mats.
	cv::Mat colonBase = load_template(dir, lang, "colon");
	std::vector<cv::Mat> digitBase;
	for (int v = 0; v <= 9; ++v) {
		digitBase.push_back(load_template(dir, lang, "digit" + std::to_string(v)));
	}

	PhaseTimer timer;

	// Wrap the caller's BGRA buffer (no copy) and convert to grayscale once;
	// every template is matched against this single-channel frame.
	const cv::Mat bgraFrame((int)height, (int)width, CV_8UC4, const_cast<uint8_t *>(bgra));
	cv::Mat frame;
	cv::cvtColor(bgraFrame, frame, cv::COLOR_BGRA2GRAY);
	timer.lap("grayscale");

	// The mission/part/difficulty labels always sit in the upper-left of the
	// stats overlay, so their template matching only needs the top-left corner
	// of the frame. Restricting the search to this region cuts the per-call
	// matchTemplate cost roughly in proportion to its area. The glyph search
	// further down still runs on the whole frame, since times can appear lower
	// in the panel. These matches do not need frame coordinates, so the smaller
	// origin requires no offset bookkeeping.
	const cv::Mat labelRegion = frame(cv::Rect(0, 0, (int)(frame.cols * kLabelRegionW), (int)(frame.rows * kLabelRegionH)));

	// Determine the global scale from mission glyphs by anchoring on ':' in the
	// label region and selecting the strongest single digit immediately left of
	// that colon. This avoids matching large mission templates across scales.
	double globalScale = 1.0;
	double bestMissionScore = kGlyphThreshold;
	for (double scale : kScales) {
		cv::Mat colonTmpl = scaled(colonBase, scale);
		std::vector<cv::Mat> digitTmpls;
		digitTmpls.reserve(10);
		for (int v = 0; v <= 9; ++v) {
			digitTmpls.push_back(scaled(digitBase[v], scale));
		}

		FoundMission found = find_mission_from_colons(labelRegion, colonTmpl, digitTmpls);
		if (found.score >= bestMissionScore) {
			bestMissionScore = found.score;
			globalScale = scale;
			result.mission = found.mission;
		}
		// The scales beyond the first exist only to recover from a capture
		// resolution that differs from the templates' authoring resolution. A
		// near-perfect match means we have already found the right scale, so the
		// remaining scales cannot improve on it -- stop early. In the common
		// native-resolution case this skips ~90% of the scale search.
		if (bestMissionScore >= kStrongLabel) {
			break;
		}
	}
	timer.lap("mission scale search");

	// Remaining labels are matched at the established scale.
	result.part = best_label(labelRegion, parts, globalScale, kLabelThreshold);
	timer.lap("part labels");
	result.difficulty = best_label(labelRegion, diffs, globalScale, kLabelThreshold);
	timer.lap("difficulty labels");

	// Locate the digit and colon glyphs at the same scale.
	cv::Mat colonTmpl = scaled(colonBase, globalScale);
	std::vector<cv::Mat> digitTmpls;
	int digitWidthSum = 0;
	for (int v = 0; v <= 9; ++v) {
		cv::Mat t = scaled(digitBase[v], globalScale);
		digitWidthSum += t.cols;
		digitTmpls.push_back(t);
	}
	timer.lap("load glyph templates");

	if (colonTmpl.empty() || digitWidthSum == 0) {
		return result; // no glyph templates: labels only
	}

	const int colonW = colonTmpl.cols;
	const int colonH = colonTmpl.rows;
	const int digitW = digitWidthSum / 10; // representative digit width
	const int digitH = digitTmpls[0].rows;

	const int colonX0 = (int)(frame.cols * kColonRegionX);
	const int colonY0 = (int)(frame.rows * kColonRegionY);
	const cv::Mat colonRegion =
	    frame(cv::Rect(colonX0, colonY0, (int)(frame.cols * kColonRegionW), (int)(frame.rows * kColonRegionH)));
	std::vector<Detection> colons;
	collect_detections(colonRegion, colonTmpl, kGlyphThreshold, 0, colons);
	// Offset back into frame coordinates: the digit bands below crop from the
	// full frame and the assembly compares colon/digit positions directly.
	for (Detection &c : colons) {
		c.x += colonX0;
		c.y += colonY0;
	}
	colons = suppress(std::move(colons), colonW, colonH, 0.5);
	timer.lap("colon detection");

	// A valid time is two digits on each side of a colon, on the colon's line,
	// so the only digits that matter live in a narrow band around each colon.
	// Matching the ten digit templates against just those bands -- rather than
	// the whole frame -- is the dominant cost saving here. Detections are
	// offset back into frame coordinates so the assembly below is unchanged.
	// Bands of neighbouring colons may overlap and double-detect a digit; the
	// suppress() pass collapses those duplicates.
	const int bandPadX = digitW * 3; // room for two digits plus gaps each side
	const int bandPadY = digitH;     // slack for digit/colon height mismatch
	std::vector<Detection> digits;
	for (const Detection &colon : colons) {
		const int x0 = std::max(0, colon.x - bandPadX);
		const int y0 = std::max(0, colon.y - bandPadY);
		const int x1 = std::min(frame.cols, colon.x + colonW + bandPadX);
		const int y1 = std::min(frame.rows, colon.y + colonH + bandPadY);
		const cv::Mat roi = frame(cv::Rect(x0, y0, x1 - x0, y1 - y0));
		// Match the ten digit templates against this band in parallel; each
		// value collects into its own bucket, so the tasks never touch shared
		// state.
		std::vector<std::vector<Detection>> perValue(10);
		cv::parallel_for_(cv::Range(0, 10), [&](const cv::Range &r) {
			for (int v = r.start; v < r.end; ++v) {
				collect_detections(roi, digitTmpls[v], kGlyphThreshold, v, perValue[v]);
			}
		});
		for (std::vector<Detection> &bucket : perValue) {
			for (Detection d : bucket) {
				d.x += x0;
				d.y += y0;
				digits.push_back(d);
			}
		}
	}
	// Suppress across all digit values so overlapping matches of different
	// digits collapse to the single strongest reading.
	digits = suppress(std::move(digits), digitW, digitH, 0.5);
	timer.lap("digit detection");

	// Assemble "mm:ss" readings: a valid time is two digits immediately to the
	// left of a colon and two immediately to its right, all on the same line.
	// Anchoring on the colon and demanding adjacent digits on both sides
	// rejects label colons ("Time:", "Accuracy:") and non-time figures
	// ("0.0%", "0 (0%)") that share the screen.
	std::vector<FoundTime> times;
	for (const Detection &colon : colons) {
		const double colonCenterY = colon.y + colonH / 2.0;

		std::vector<Detection> right, left;
		for (const Detection &d : digits) {
			if (std::abs((d.y + digitH / 2.0) - colonCenterY) >= digitH * 0.35) {
				continue; // not on the colon's text line
			}
			if (d.x >= colon.x + colonW * 0.3) {
				right.push_back(d);
			} else if (d.x + d.w <= colon.x + colonW * 0.7) {
				left.push_back(d);
			}
		}
		if (right.size() < 2 || left.size() < 2) {
			continue;
		}
		std::sort(right.begin(), right.end(), [](const Detection &a, const Detection &b) { return a.x < b.x; });
		std::sort(left.begin(), left.end(), [](const Detection &a, const Detection &b) { return a.x > b.x; });

		const Detection &r0 = right[0];
		const Detection &r1 = right[1];
		const Detection &l0 = left[0];
		const Detection &l1 = left[1];

		// Spacing checks (in digit-width fractions): the inner digits must hug
		// the colon and the outer digits must abut the inner ones.
		const double adjRight = r0.x - (colon.x + colonW);
		const double adjLeft = colon.x - (l0.x + l0.w);
		const double gapRight = r1.x - (r0.x + r0.w);
		const double gapLeft = l0.x - (l1.x + l1.w);
		if (adjRight < -digitW * 0.4 || adjRight > digitW * 0.6 || adjLeft < -digitW * 0.4 ||
		    adjLeft > digitW * 0.6 || std::abs(gapRight) > digitW * 0.6 || std::abs(gapLeft) > digitW * 0.6) {
			continue;
		}

		const int minutes = l1.value * 10 + l0.value;
		const int seconds = r0.value * 10 + r1.value;
		times.push_back({colon.y, colon.x, minutes * 60 + seconds});
	}

	// Order top-to-bottom (bucketed by line) then left-to-right.
	const double lineBucket = digitH * 0.5;
	std::sort(times.begin(), times.end(), [lineBucket](const FoundTime &a, const FoundTime &b) {
		int ra = (int)std::lround(a.y / lineBucket);
		int rb = (int)std::lround(b.y / lineBucket);
		if (ra != rb) {
			return ra < rb;
		}
		return a.x < b.x;
	});

	if (!times.empty()) {
		int32_t *out = (int32_t *)std::malloc(times.size() * sizeof(int32_t));
		if (out) {
			for (size_t i = 0; i < times.size(); ++i) {
				out[i] = times[i].seconds;
			}
			result.times = out;
			result.times_len = times.size();
		}
	}
	timer.lap("time assembly");

	return result;
}

void ge_cv_match_result_free(ge_level_match_result_t *result) {
	if (!result) {
		return;
	}
	std::free(result->times);
	result->times = nullptr;
	result->times_len = 0;
}
