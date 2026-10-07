#target aftereffects

// Synthetic alpha reference; never executes supplied JSX or saves the project.
// Supply {output: new JSON path, amounts: 1..16 values in 0..2048} explicitly.
// geometry: "square" (default) or "wide-edge" (isolates large feather kernels).
(function () {
  var config = $.global.LIBRE_EFFECTS_MASK_FEATHER_REFERENCE;
  if (!config || !(config.amounts instanceof Array) || !config.amounts.length || config.amounts.length > 16)
    throw new Error("Supply 1..16 mask feather amounts");
  if (typeof config.output !== "string" || !config.output.length || config.output.length > 8192)
    throw new Error("Supply an explicit bounded JSON output path");
  var output = new File(config.output);
  if (output.exists || !output.parent.exists) throw new Error("Supply a new JSON output in an existing directory");
  var geometry = typeof config.geometry === "undefined" ? "square" : config.geometry;
  if (geometry !== "square" && geometry !== "wide-edge") throw new Error("Unknown feather reference geometry");
  var files = [];
  for (var i = 0; i < config.amounts.length; i++) {
    var amount = config.amounts[i];
    if (typeof amount !== "number" || !isFinite(amount) || amount < 0 || amount > 2048)
      throw new Error("Feather amounts must be finite and within 0..2048");
    var file = new File(output.fsName + "." + i + ".png");
    if (file.exists) throw new Error("Feather PNG output already exists");
    files.push(file);
  }
  var before = app.project.numItems;
  var comp = null, solidSource = null, records = [];
  try {
    var width = geometry === "wide-edge" ? 8192 : 512;
    var height = geometry === "wide-edge" ? 64 : 512;
    // Sample the middle of a tall source to keep the source's vertical bounds
    // far from the edge being measured, including for large feather amounts.
    var sourceHeight = geometry === "wide-edge" ? 8192 : height;
    comp = app.project.items.addComp("Libre Effects temporary mask feather probe", width, height, 1, 1, 30);
    comp.resolutionFactor = [1, 1];
    var layer = comp.layers.addSolid([1, 1, 1], "synthetic feather source", width, sourceHeight, 1, 1);
    solidSource = layer.source;
    var mask = layer.property("ADBE Mask Parade").addProperty("ADBE Mask Atom");
    var shape = new Shape();
    shape.vertices = geometry === "wide-edge"
      ? [[4096, -8192], [16384, -8192], [16384, 16384], [4096, 16384]]
      : [[128, 128], [384, 128], [384, 384], [128, 384]];
    shape.inTangents = [[0, 0], [0, 0], [0, 0], [0, 0]];
    shape.outTangents = [[0, 0], [0, 0], [0, 0], [0, 0]];
    shape.closed = true;
    mask.property("ADBE Mask Shape").setValue(shape);
    for (var i = 0; i < config.amounts.length; i++) {
      mask.property("ADBE Mask Feather").setValue([config.amounts[i], config.amounts[i]]);
      comp.saveFrameToPng(0, files[i]);
      var deadline = new Date().getTime() + 30000;
      var fresh = new File(files[i].fsName);
      while ((!fresh.exists || !fresh.length) && new Date().getTime() < deadline) {
        $.sleep(50);
        fresh = new File(files[i].fsName);
      }
      if (!fresh.exists || !fresh.length) throw new Error("Feather PNG was not committed");
      records.push('{"index":' + i + ',"feather":' + config.amounts[i] + '}');
    }
  } finally {
    if (comp) comp.remove();
    if (solidSource) solidSource.remove();
  }
  output.encoding = "UTF-8";
  if (!output.open("w")) throw new Error("Cannot create feather reference output");
  try {
    if (!output.write('{"before":' + before + ',"after":' + app.project.numItems + ',"bits_per_channel":' + app.project.bitsPerChannel + ',"geometry":"' + geometry + '","width":' + width + ',"height":' + height + ',"source_height":' + sourceHeight + ',"records":[' + records.join(",") + ']}'))
      throw new Error("Feather reference write failed");
  } finally { output.close(); }
}());
