#target aftereffects

// Explicit local PNG cases, for visual qualification against the real renderer.
// This diagnostic method is capability-checked; no queue, project or preferences
// are saved. Restore the composition's preview resolution even after failure.
// This diagnostic PNG is not guaranteed to contain straight RGB. For alpha
// qualification use ae-render-queue-reference.jsx with a verified Straight
// RGBA8 output template; do not silently unassociate these diagnostic bytes.
(function () {
  var cases = $.global.LIBRE_EFFECTS_FRAME_REFERENCE;
  if (!(cases instanceof Array) || !cases.length || cases.length > 64) throw new Error("Supply 1..64 frame cases");
  var seen = {};
  // Validate the complete request before capturing the first frame.
  for (var i = 0; i < cases.length; i++) {
    var item = cases[i];
    if (!item || typeof item.compositionId !== "number" || !isFinite(item.compositionId) || item.compositionId <= 0 || item.compositionId !== Math.floor(item.compositionId))
      throw new Error("Frame case requires a positive composition identity");
    var comp = app.project.itemByID(item.compositionId);
    if (!(comp instanceof CompItem) || typeof item.time !== "number" || !isFinite(item.time) || item.time < 0 || item.time >= comp.duration)
      throw new Error("Invalid frame case");
    if (typeof item.output !== "string" || !item.output.length || item.output.length > 8192)
      throw new Error("Supply an explicit bounded PNG output path");
    var output = new File(item.output);
    var key = "file:" + output.fsName.toLowerCase();
    if (output.exists || !output.parent.exists || seen[key]) throw new Error("Supply distinct new PNG outputs in existing directories");
    seen[key] = true;
    if (typeof comp.saveFrameToPng !== "function") throw new Error("This AE version has no diagnostic PNG capture method");
  }
  function completePng(path) {
    var file = new File(path);
    if (!file.exists || file.length < 20) return false;
    file.encoding = "BINARY";
    if (!file.open("r")) return false;
    try {
      if (!file.seek(file.length - 12, 0)) return false;
      return file.read(12) === "\x00\x00\x00\x00IEND\xAE\x42\x60\x82";
    } finally { file.close(); }
  }
  for (var i = 0; i < cases.length; i++) {
    var item = cases[i];
    var comp = app.project.itemByID(item.compositionId);
    var output = new File(item.output);
    if (output.exists) throw new Error("Frame output already exists");
    if (typeof comp.saveFrameToPng !== "function") throw new Error("This AE version has no diagnostic PNG capture method");
    var resolution = comp.resolutionFactor;
    try {
      comp.resolutionFactor = [1, 1];
      comp.saveFrameToPng(item.time, output);
      // AE can return before its worker commits the PNG. A nonempty file may
      // still be incomplete, so wait for its PNG end chunk using fresh objects.
      var deadline = new Date().getTime() + 30000;
      while (!completePng(item.output) && new Date().getTime() < deadline) $.sleep(50);
      if (!completePng(item.output)) throw new Error("AE did not complete the requested PNG within 30 seconds");
    } finally { comp.resolutionFactor = resolution; }
  }
}());
