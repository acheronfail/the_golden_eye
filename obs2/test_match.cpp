// Standalone CLI for exercising ge_cv_match_level() outside of OBS.
//
//   test_match path/to/screenshot.png [lang] [templates_dir]
//
// Loads the given image, converts it to the BGRA layout the plugin feeds the
// matcher, runs ge_cv_match_level(), and prints the match result to stdout.
// `lang` defaults to "en" and `templates_dir` to the templates/ directory that
// ships alongside this source.

#include <cstdio>
#include <cstdlib>
#include <string>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

#include "cv_wrapper.h"

int main(int argc, char **argv) {
	if (argc < 2) {
		std::fprintf(stderr, "usage: %s path/to/png [lang] [templates_dir]\n", argv[0]);
		return 2;
	}

	const char *image_path = argv[1];
	const char *lang = argc > 2 ? argv[2] : "en";
	const std::string templates_dir = argc > 3 ? argv[3] : (std::string(GE_TEMPLATES_DIR));

	// Benchmarking hook: GE_CV_THREADS caps OpenCV's internal thread pool so we
	// can see how much matchTemplate already parallelizes on its own.
	if (const char *t = std::getenv("GE_CV_THREADS")) {
		cv::setNumThreads(std::atoi(t));
		std::fprintf(stderr, "[test_match] cv::setNumThreads(%d)\n", std::atoi(t));
	}

	// Load as BGR, then add an opaque alpha channel so the buffer matches the
	// BGRA frames ge_cv_match_level() expects from OBS.
	cv::Mat bgr = cv::imread(image_path, cv::IMREAD_COLOR);
	if (bgr.empty()) {
		std::fprintf(stderr, "error: could not read image '%s'\n", image_path);
		return 1;
	}

	cv::Mat bgra;
	cv::cvtColor(bgr, bgra, cv::COLOR_BGR2BGRA);
	if (!bgra.isContinuous()) {
		bgra = bgra.clone();
	}

	ge_level_match_result_t result =
	    ge_cv_match_level(bgra.data, (uint32_t)bgra.cols, (uint32_t)bgra.rows, lang, templates_dir.c_str());

	std::printf("opencv:     %s\n", ge_cv_version());
	std::printf("image:      %s (%dx%d)\n", image_path, bgra.cols, bgra.rows);
	std::printf("lang:       %s\n", lang);
	std::printf("templates:  %s\n", templates_dir.c_str());
	std::printf("mission:    %d\n", result.mission);
	std::printf("part:       %d\n", result.part);
	std::printf("difficulty: %d\n", result.difficulty);

	std::printf("times:      %zu\n", result.times_len);
	for (size_t i = 0; i < result.times_len; ++i) {
		int seconds = result.times[i];
		std::printf("  [%zu] %d (%d:%02d)\n", i, seconds, seconds / 60, seconds % 60);
	}

	ge_cv_match_result_free(&result);
	return 0;
}
