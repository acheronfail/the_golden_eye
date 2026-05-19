import { Log } from "@u4/opencv-build";

// Must run before importing opencv4nodejs to silence init logs.
Log.silence = process.env.DEBUG ? false : true;

const { default: cv } = await import("@u4/opencv4nodejs");
export default cv;
