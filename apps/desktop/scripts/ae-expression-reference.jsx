#target aftereffects

// Read-only AE oracle for compare_reference. Caller supplies a new output path
// and explicit { compositionId, time } cases. No project is saved or modified.
(function () {
  var config = $.global.LIBRE_EFFECTS_EXPRESSION_REFERENCE;
  if (!config || !config.cases || config.cases.length > 256) throw new Error("Supply at most 256 reference cases");
  var output = new File(config.output);
  if (output.exists) throw new Error("Reference output already exists");
  function quote(text) {
    return '"' + String(text).replace(/["\\\u0000-\u001f]/g, function (character) {
      return "\\u" + ("0000" + character.charCodeAt(0).toString(16)).slice(-4);
    }) + '"';
  }
  function json(data) {
    if (data === null || typeof data === "undefined") return "null";
    if (typeof data === "string") return quote(data);
    if (typeof data === "boolean") return data ? "true" : "false";
    if (typeof data === "number") {
      if (!isFinite(data)) throw new Error("Nonfinite reference value");
      return String(data);
    }
    var entries = [];
    if (data instanceof Array) {
      for (var i = 0; i < data.length; i++) entries.push(json(data[i]));
      return "[" + entries.join(",") + "]";
    }
    for (var name in data) if (data.hasOwnProperty(name)) entries.push(quote(name) + ":" + json(data[name]));
    return "{" + entries.join(",") + "}";
  }
  function convert(data, spatialDimensions) {
    if (data instanceof TextDocument) return data.text;
    if (data instanceof Shape) return {
      vertices: data.vertices, in_tangents: data.inTangents,
      out_tangents: data.outTangents, closed: data.closed
    };
    // A 2D layer's scripting API reports a dormant third transform axis. The
    // bounded native 2D host exposes its two active axes, without modifying
    // source code. Only transform Position/Scale use this explicit projection.
    if (spatialDimensions === 2) return [data[0], data[1]];
    return data;
  }
  var result = [];
  for (var c = 0; c < config.cases.length; c++) {
    var request = config.cases[c];
    var comp = app.project.itemByID(request.compositionId);
    if (!(comp instanceof CompItem) || !isFinite(request.time)) throw new Error("Invalid composition/time case");
    if (comp.frameRate !== Math.round(comp.frameRate)) throw new Error("Supply an explicit rational frame-rate adapter for fractional FPS");
    var snapshot = { id: comp.id, width: comp.width, height: comp.height,
      duration: comp.duration, frame_rate: { numerator: comp.frameRate, denominator: 1 },
      time: request.time, sources: [], layers: [] };
    var expected = [];
    function capture(property, layer, addressProperty, dimensions) {
      var program = null;
      if (property.canSetExpression && property.expression) {
        var sourceId = -1;
        for (var s = 0; s < snapshot.sources.length; s++) if (snapshot.sources[s] === property.expression) sourceId = s;
        if (sourceId < 0) { sourceId = snapshot.sources.length; snapshot.sources.push(property.expression); }
        program = { source_id: sourceId, enabled: property.expressionEnabled };
        var bindings = config.localBindings || [];
        for (var b = 0; b < bindings.length; b++) {
          if (bindings[b].compositionId === comp.id && bindings[b].layerId === layer.id
              && bindings[b].property === addressProperty) program.local_bindings = bindings[b].names;
        }
      }
      if (program && program.enabled) expected.push([
        { composition: comp.id, layer: layer.id, property: addressProperty },
        convert(property.valueAtTime(request.time, false), dimensions)
      ]);
      return { authored_value: convert(property.valueAtTime(request.time, true), dimensions), expression: program };
    }
    for (var l = 1; l <= comp.numLayers; l++) {
      var layer = comp.layer(l);
      var transform = layer.property("ADBE Transform Group");
      var dimensions = layer.threeDLayer ? 3 : 2;
      var item = { id: layer.id, name: layer.name, start_time: layer.startTime,
        in_point: layer.inPoint, out_point: layer.outPoint,
        position: capture(transform.property("ADBE Position"), layer, "Position", dimensions),
        scale: capture(transform.property("ADBE Scale"), layer, "Scale", dimensions),
        opacity: capture(transform.property("ADBE Opacity"), layer, "Opacity", 0),
        source_text: null, masks: [], sliders: [], markers: [] };
      var textGroup = layer.property("ADBE Text Properties");
      if (textGroup) item.source_text = capture(textGroup.property("ADBE Text Document"), layer, "SourceText", 0);
      var effects = layer.property("ADBE Effect Parade");
      if (effects) for (var e = 1; e <= effects.numProperties; e++) {
        var effect = effects.property(e);
        if (effect.matchName === "ADBE Slider Control") item.sliders.push({ name: effect.name,
          property: capture(effect.property("ADBE Slider Control-0001"), layer, { Slider: effect.name }, 0) });
      }
      var masks = layer.property("ADBE Mask Parade");
      if (masks) for (var m = 1; m <= masks.numProperties; m++) item.masks.push({ id: m,
        property: capture(masks.property(m).property("ADBE Mask Shape"), layer, { MaskPath: m }, 0) });
      var marker = layer.property("ADBE Marker");
      if (marker) for (var k = 1; k <= marker.numKeys; k++) item.markers.push({ time: marker.keyTime(k), comment: marker.keyValue(k).comment });
      snapshot.layers.push(item);
    }
    result.push({ snapshot: snapshot, expected: expected });
  }
  output.encoding = "UTF-8";
  if (!output.open("w")) throw new Error("Cannot write expression reference");
  try { if (!output.write(json(result))) throw new Error("Expression reference write failed"); }
  finally { output.close(); }
}());
