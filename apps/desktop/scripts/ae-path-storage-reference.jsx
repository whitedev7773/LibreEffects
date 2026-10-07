#target aftereffects

// Independent zero-handle polygon probe; no supplied project program is run.
// Supply {output: new JSON path, values: finite coordinates} explicitly.
(function () {
  var config = $.global.LIBRE_EFFECTS_PATH_STORAGE_REFERENCE;
  if (!config || !(config.values instanceof Array) || !config.values.length || config.values.length > 128)
    throw new Error("Supply 1..128 finite coordinate values");
  for (var i = 0; i < config.values.length; i++) {
    var value = config.values[i];
    if (typeof value !== "number" || !isFinite(value) || Math.abs(value) > 1000000)
      throw new Error("Coordinates must be finite and within +/-1000000");
  }
  var output = new File(config.output);
  if (output.exists) throw new Error("Path storage output already exists");
  var before = app.project.numItems;
  var comp = null;
  var solidSource = null;
  var records = [];
  try {
    comp = app.project.items.addComp("Libre Effects temporary polygon storage probe", 16, 16, 1, 1, 30);
    var solid = comp.layers.addSolid([1, 1, 1], "probe", 16, 16, 1, 1);
    solidSource = solid.source;
    var property = solid.property("ADBE Mask Parade").addProperty("ADBE Mask Atom").property("ADBE Mask Shape");
    for (var i = 0; i < config.values.length; i++) {
      var value = config.values[i];
      property.expression = "createPath([[" + value + "," + value + "],[1,1],[0,1]],[],[],true)";
      var path = property.valueAtTime(0, false);
      if (property.expressionError) throw new Error(property.expressionError);
      records.push('{"input":' + value + ',"vertices":[[' + path.vertices[0][0] + ',' + path.vertices[0][1] + '],[1,1],[0,1]]}');
    }
  } finally {
    if (comp) comp.remove();
    if (solidSource) solidSource.remove();
  }
  output.encoding = "UTF-8";
  if (!output.open("w")) throw new Error("Cannot create path storage output");
  try {
    if (!output.write('{"before":' + before + ',"after":' + app.project.numItems + ',"records":[' + records.join(",") + ']}'))
      throw new Error("Path storage output write failed");
  } finally { output.close(); }
}());
