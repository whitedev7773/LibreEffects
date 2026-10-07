#target aftereffects

// Owned synthetic 8-bpc normal-compositing probe. No supplied program is run and
// the original project is never saved. Explicitly supply a new empty directory,
// byte RGB foreground, and 1..256 opacity percentages; retain receipts and PNGs.
(function () {
  var config = $.global.LIBRE_EFFECTS_OPACITY_REFERENCE;
  if (!config || typeof config.directory !== "string" || config.directory.length > 8192)
    throw new Error("Supply an explicit opacity probe directory");
  var folder = new Folder(config.directory);
  if (!folder.exists || folder.getFiles().length) throw new Error("Supply an existing empty directory");
  if (app.project.bitsPerChannel !== 8) throw new Error("Opacity byte probe requires the existing project to be 8 bpc");
  var colors = config.foreground, values = config.opacity;
  if (!(colors instanceof Array) || colors.length !== 3 || !(values instanceof Array) || !values.length || values.length > 256)
    throw new Error("Supply three byte colors and 1..256 opacity percentages");
  for (var c = 0; c < 3; c++)
    if (typeof colors[c] !== "number" || !isFinite(colors[c]) || colors[c] < 0 || colors[c] > 255 || colors[c] !== Math.floor(colors[c]))
      throw new Error("Foreground colors must be bytes");
  for (var i = 0; i < values.length; i++)
    if (typeof values[i] !== "number" || !isFinite(values[i]) || values[i] < 0 || values[i] > 100)
      throw new Error("Opacity percentages must be finite and in 0..100");
  var capture = new File(new File($.fileName).parent.fsName + "/ae-frame-reference.jsx");
  if (!capture.exists) throw new Error("Missing owned frame-reference helper");
  var before = app.project.numItems, comp = null, sources = [], completed = 0;
  var previous = $.global.LIBRE_EFFECTS_FRAME_REFERENCE;
  function write(name, value) {
    var file = new File(folder.fsName + "/" + name);
    file.encoding = "UTF-8";
    if (!file.open("w")) throw new Error("Cannot record opacity probe result");
    try { file.write(JSON.stringify(value)); } finally { file.close(); }
  }
  try {
    comp = app.project.items.addComp("Libre Effects owned opaque opacity probe", 1028, 16, 1, values.length / 30, 30);
    comp.bgColor = [0, 0, 0];
    for (var i = 0; i < 256; i++) {
      var gray = i / 255;
      var layer = comp.layers.addSolid([gray, gray, gray], "Owned gray " + i, 4, 16, 1);
      sources.push(layer.source);
      layer.property("ADBE Transform Group").property("ADBE Position").setValue([2 + i * 4, 8]);
    }
    var foreground = comp.layers.addSolid([colors[0] / 255, colors[1] / 255, colors[2] / 255], "Owned foreground", 1028, 16, 1);
    sources.push(foreground.source);
    var opacity = foreground.property("ADBE Transform Group").property("ADBE Opacity");
    for (var i = 0; i < values.length; i++) {
      opacity.setValueAtTime(i / 30, values[i]);
      opacity.setInterpolationTypeAtKey(i + 1, KeyframeInterpolationType.HOLD, KeyframeInterpolationType.HOLD);
    }
    for (var i = 0; i < values.length; i++) {
      if (new File(folder.fsName + "/cancel").exists) throw new Error("Owned opacity probe canceled");
      $.global.LIBRE_EFFECTS_FRAME_REFERENCE = [{compositionId: comp.id, time: i / 30, output: folder.fsName + "/frame-" + ("000" + i).slice(-3) + ".png"}];
      $.evalFile(capture);
      write("progress.json", {completed: ++completed, total: values.length, original_saved: false});
    }
  } finally {
    $.global.LIBRE_EFFECTS_FRAME_REFERENCE = previous;
    if (comp) comp.remove();
    for (var i = sources.length - 1; i >= 0; i--) sources[i].remove();
  }
  write("receipt.json", {before: before, after: app.project.numItems, frames: completed, foreground: colors, opacity: values, width: 1028, height: 16, gray_levels: 256, transparent_strip_x: 1026, bits_per_channel: app.project.bitsPerChannel, background: [0, 0, 0], original_saved: false});
})();
