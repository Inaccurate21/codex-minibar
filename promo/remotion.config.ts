import { Config } from "@remotion/cli/config";

Config.setEntryPoint("src/index.ts");
// Render through D3D on the GPU instead of the default software GL.
Config.setChromiumOpenGlRenderer("angle");
// Opaque videos; transparent clips request PNG frames in render.sh.
Config.setVideoImageFormat("jpeg");
Config.setJpegQuality(95);
