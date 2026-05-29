#include "cv_wrapper.h"

#include <cstdlib>
#include <cstring>

#include <opencv2/core.hpp>
#include <opencv2/imgproc.hpp>

const char *ge_cv_version(void) { return CV_VERSION; }

/*
  TODO: using `obs2/templates`, write a function which will quickly match them and return
  a ge_level_match_result_t:
  {
    // the mission template match, a number between 1-9 (or -1 if no match)
    // use `obs2/templates/$LANG-mission\d.png` for the templates
    mission: int,
    // the part template match, a number between 1-5 (or -1 if no match)
    // use `obs2/templates/$LANG-part\d.png` for the templates
    part: int,
    // the difficulty template match, a number between 1-4 (or -1 if no match)
    // use `obs2/templates/$LANG-diff\d.png` for the templates
    difficulty: int,
    // all times matched on screen (using digit templates), need to only match
    // digits in the format of "mm:ss" (and not match the other numbers on screen)
    // the return type is the matched times in seconds, so "01:30" would be returned as 90.
    // The order of the array should be the way they appear on screen, from top to bottom, left to right.
    // use `obs2/templates/$LANG-{digit\d,colon}.png` for the templates
    times: Array<int>
  }
*/
