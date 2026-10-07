#target aftereffects

// Explicit full-resolution Straight RGBA8 PNGs from the installed AE renderer.
// Require a stopped empty queue and a verified existing output template. Create
// only owned queue items; never save the project or change its authored items.
(function () {
  var config = $.global.LIBRE_EFFECTS_RENDER_QUEUE_REFERENCE;
  if (!config || typeof config.directory !== "string" || !config.directory.length || config.directory.length > 8192)
    throw new Error("Supply an explicit capture directory");
  var folder = new Folder(config.directory);
  if (!folder.exists || folder.getFiles().length) throw new Error("Supply an existing empty capture directory");
  if (typeof config.template !== "string" || !config.template.length || config.template.length > 256)
    throw new Error("Supply an explicit installed Straight RGBA8 PNG template");
  var cases = config.cases;
  if (!(cases instanceof Array) || !cases.length || cases.length > 64)
    throw new Error("Supply 1..64 composition/frame cases");
  if (!app.project || app.project.bitsPerChannel !== 8)
    throw new Error("The existing project must be 8 bpc");
  var rq = app.project.renderQueue;
  if (rq.rendering || rq.numItems !== 0) throw new Error("Render queue must be stopped and empty");
  var pixels = 0;
  // Validate the complete job before adding the first queue item or output.
  for (var i = 0; i < cases.length; i++) {
    var c = cases[i];
    if (!c || typeof c.compositionId !== "number" || !isFinite(c.compositionId) || c.compositionId <= 0 || c.compositionId !== Math.floor(c.compositionId))
      throw new Error("Supply a positive composition identity");
    var comp = app.project.itemByID(c.compositionId);
    if (!(comp instanceof CompItem) || typeof c.frame !== "number" || !isFinite(c.frame) || c.frame < 0 || c.frame !== Math.floor(c.frame) || c.frame * comp.frameDuration >= comp.duration)
      throw new Error("Frame must be an integer inside its composition");
    if (comp.width > 8192 || comp.height > 8192 || comp.width * comp.height > 33554432)
      throw new Error("Capture supports 8192 pixels per axis and 32 megapixels per frame");
    pixels += comp.width * comp.height;
    if (pixels > 134217728) throw new Error("Capture job exceeds 128 megapixels");
  }
  var before = app.project.numItems, items = [], measurements = [];
  try {
    for (var i = 0; i < cases.length; i++) {
      var c = cases[i], comp = app.project.itemByID(c.compositionId);
      var item = rq.items.add(comp);
      items.push(item);
      item.applyTemplate("Best Settings");
      item.timeSpanStart = c.frame * comp.frameDuration;
      item.timeSpanDuration = comp.frameDuration;
      var om = item.outputModule(1), templates = om.templates, found = false;
      for (var j = 0; j < templates.length; j++) if (templates[j] === config.template) found = true;
      if (!found) throw new Error("Requested output template is not installed");
      om.applyTemplate(config.template);
      var prefix = "case-" + ("000" + i).slice(-3) + "_";
      om.file = new File(folder.fsName + "/" + prefix + "[#####].png");
      var actual = om.getSettings(GetSettingsFormat.STRING);
      var render = item.getSettings(GetSettingsFormat.STRING);
      if (actual.Format !== "PNG Sequence" || actual.Color !== "Straight (Unmatted)" || actual.Channels !== "RGB + Alpha" || actual.Depth !== "Millions of Colors+" || actual.Resize !== "false" || actual.Crop !== "false" || actual["Use Region of Interest"] !== "false" || actual["Post-Render Action"] !== "None" || actual["Output Audio"] !== "Off" || actual["Resize to"].x !== comp.width || actual["Resize to"].y !== comp.height || render.Resolution !== "Full" || render.Quality !== "Best")
        throw new Error("Actual output settings do not provide unmodified full-resolution Straight RGBA8 PNGs");
      measurements.push({composition: comp.id, frame: c.frame, width: comp.width, height: comp.height, prefix: prefix, output_settings: actual, render_settings: render});
    }
    rq.render();
    for (var i = 0; i < items.length; i++) {
      if (items[i].status !== RQItemStatus.DONE) throw new Error("Reference render did not complete");
      var files = folder.getFiles(measurements[i].prefix + "*.png");
      if (files.length !== 1 || !(files[0] instanceof File) || files[0].length < 20)
        throw new Error("Reference render did not produce exactly one nonempty PNG");
      measurements[i].reference = files[0].fsName;
      measurements[i].bytes = files[0].length;
      delete measurements[i].prefix;
    }
  } finally {
    for (var i = items.length - 1; i >= 0; i--) items[i].remove();
  }
  if (app.project.numItems !== before || rq.numItems !== 0)
    throw new Error("Authored item counts or original queue changed during capture");
  var receipt = new File(folder.fsName + "/receipt.json");
  receipt.encoding = "UTF-8";
  if (!receipt.open("w")) throw new Error("Cannot create capture receipt");
  try {
    receipt.write(JSON.stringify({version: 1, encoding: "straight-rgba8", template: config.template, bits_per_channel: app.project.bitsPerChannel, items_before: before, items_after: app.project.numItems, queue_before: 0, queue_after: rq.numItems, original_saved: false, measurements: measurements}));
  } finally { receipt.close(); }
})();
