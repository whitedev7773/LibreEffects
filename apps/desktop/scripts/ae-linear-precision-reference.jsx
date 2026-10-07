#target aftereffects

// A temporary synthetic composition isolates expression math from Shape storage.
// It is removed in finally; the open source project and its file are never saved.
(function () {
  var config = $.global.LIBRE_EFFECTS_LINEAR_REFERENCE;
  if (!config || !(config.times instanceof Array) || config.times.length > 64)
    throw new Error("Supply at most 64 precision sample times");
  var output = new File(config.output);
  if (output.exists) throw new Error("Precision output already exists");
  var comp = null;
  var solidSource = null;
  var records = [];
  try {
    comp = app.project.items.addComp("Libre Effects temporary precision probe", 16, 16, 1, 600, 60);
    var text = comp.layers.addText("probe").property("ADBE Text Properties").property("ADBE Text Document");
    text.expression = 'JSON.stringify({linear:linear(time,0,262.8,-400,65),arithmetic:-400+(time/262.8)*465,weighted:-400*(1-time/262.8)+65*(time/262.8)})';
    var solid = comp.layers.addSolid([1, 1, 1], "probe", 16, 16, 1);
    solidSource = solid.source;
    var path = solid.property("ADBE Mask Parade").addProperty("ADBE Mask Atom").property("ADBE Mask Shape");
    var programs = [
      'createPath([[linear(time,0,262.8,-400,65),0],[1,1],[0,1]],[],[],true)',
      'createPath([[-400+(time/262.8)*465,0],[1,1],[0,1]],[],[],true)'
    ];
    for (var i = 0; i < config.times.length; i++) {
      var time = config.times[i];
      if (!isFinite(time) || time < 0 || time >= comp.duration) throw new Error("Invalid probe time");
      var math = text.valueAtTime(time, false).text;
      if (text.expressionError) throw new Error(text.expressionError);
      var stored = [];
      for (var p = 0; p < programs.length; p++) {
        path.expression = programs[p];
        stored.push(path.valueAtTime(time, false).vertices[0][0]);
        if (path.expressionError) throw new Error(path.expressionError);
      }
      records.push('{"time":' + time + ',"math":' + math + ',"linearShape":' + stored[0] + ',"arithmeticShape":' + stored[1] + '}');
    }
  } finally {
    if (comp) comp.remove();
    // addSolid creates a separate project item; deleting the composition alone
    // leaves that item behind. This item belongs exclusively to this probe.
    if (solidSource) solidSource.remove();
  }
  output.encoding = "UTF-8";
  if (!output.open("w")) throw new Error("Cannot create precision output");
  try { if (!output.write("[" + records.join(",") + "]")) throw new Error("Precision output write failed"); }
  finally { output.close(); }
}());
