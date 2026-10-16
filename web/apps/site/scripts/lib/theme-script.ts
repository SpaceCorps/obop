/** Runs in <head> before first paint: applies the saved theme and marks the page as script-enabled. */
export const THEME_SCRIPT = `(function(){var d=document.documentElement;d.classList.add("js");try{var t=localStorage.getItem("obop-theme");if(t==="light"||t==="dark")d.setAttribute("data-theme",t)}catch(e){}})()`;
