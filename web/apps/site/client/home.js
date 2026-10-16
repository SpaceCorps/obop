// The ambient ASCII field behind the home page hero (design/ascii-field.js). Decorative only:
// the canvas is aria-hidden, pauses when the tab is hidden and draws one still frame for people
// who prefer reduced motion.
import { asciiField } from "/site/ascii-field.js";

const canvas = document.querySelector("canvas.ascii");
if (canvas) asciiField(canvas);
