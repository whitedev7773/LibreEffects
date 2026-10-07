#target aftereffects

// Read-only reference capture. Run with LIBRE_EFFECTS_REFERENCE_OUTPUT set to
// a new local JSON file. This is diagnostic data, not the AE interchange format.
(function () {
  var output = $.global.LIBRE_EFFECTS_REFERENCE_OUTPUT;
  if (!output) throw new Error("Set LIBRE_EFFECTS_REFERENCE_OUTPUT before capture");
  var destination = new File(output);
  if (destination.exists) throw new Error("Reference output already exists");
  if (!app.project || !app.project.file) throw new Error("Open a saved AE project first");

  var errors = [];
  var counts = { items: 0, layers: 0, properties: 0, keys: 0 };
  function read(object, name) {
    try { return object[name]; }
    catch (error) { return null; }
  }
  function fields(object, names) {
    var result = {};
    for (var i = 0; i < names.length; i++) {
      var value = read(object, names[i]);
      result[names[i]] = typeof value === "undefined" ? null : value;
    }
    return result;
  }
  function enumeration(domain, names) {
    var result = {};
    for (var i = 0; i < names.length; i++) {
      var entry = read(domain, names[i]);
      result[names[i]] = typeof entry === "undefined" ? null : Number(entry);
    }
    return result;
  }
  function value(data, matchName) {
    if (data === null || typeof data === "undefined") return null;
    if (typeof data === "string" || typeof data === "boolean") return data;
    if (typeof data === "number") return isFinite(data) ? data : null;
    if (data instanceof Array) {
      var array = [];
      for (var i = 0; i < data.length; i++) array.push(value(data[i]));
      return array;
    }
    if (data instanceof TextDocument) {
      var document = {
        type: "text_document",
        attributes: fields(data, ["text", "font", "fontFamily", "fontStyle", "fontSize",
          "fauxBold", "fauxItalic", "allCaps", "smallCaps", "applyFill", "fillColor",
          "applyStroke", "strokeColor", "strokeWidth", "strokeOverFill", "tracking",
          "leading", "autoLeading", "baselineShift", "horizontalScale", "verticalScale",
          "justification", "pointText", "boxText", "boxTextSize", "boxTextPos",
          "direction", "digitSet", "ligature", "kerning", "autoKernType", "baselineLocs",
          "composedLineCount", "composerEngine", "everyLineComposer"])
      };
      document.characterRuns = [];
      document.characterRunState = "available";
      try {
        var previous = null;
        for (var offset = 0; offset < data.text.length;) {
          var length = 1;
          var code = data.text.charCodeAt(offset);
          if (code >= 0xd800 && code <= 0xdbff) length = 2;
          var character = data.characterRange(offset, offset + length);
          var attributes = fields(character, ["font", "fontSize", "fauxBold", "fauxItalic",
            "applyFill", "fillColor", "applyStroke", "strokeColor", "strokeWidth",
            "strokeOverFill", "tracking", "leading", "autoLeading", "baselineShift",
            "horizontalScale", "verticalScale", "kerning"]);
          var identity = json(attributes);
          if (previous && previous.identity === identity) previous.run.endUTF16 = offset + length;
          else {
            var run = { startUTF16: offset, endUTF16: offset + length, attributes: attributes };
            document.characterRuns.push(run);
            previous = { identity: identity, run: run };
          }
          offset += length;
        }
      } catch (error) {
        document.characterRunState = "unavailable";
        document.characterRunError = String(error);
      }
      return document;
    }
    if (data instanceof Shape) {
      return { type: "shape", attributes: fields(data, ["vertices", "inTangents",
        "outTangents", "closed", "featherSegLocs", "featherRelSegLocs", "featherRadii",
        "featherInterps", "featherTensions", "featherTypes", "featherRelCornerAngles"]) };
    }
    if (matchName === "ADBE Marker" || data instanceof MarkerValue) {
      return { type: "marker", attributes: fields(data, ["comment", "chapter", "url",
        "frameTarget", "cuePointName", "eventCuePoint", "duration", "label",
        "protectedRegion"]), parameters: data.getParameters() };
    }
    return { type: "opaque", description: String(data) };
  }
  function ease(property, key, incoming) {
    try {
      var source = incoming ? property.keyInTemporalEase(key) : property.keyOutTemporalEase(key);
      var result = [];
      for (var i = 0; i < source.length; i++) result.push(fields(source[i], ["speed", "influence"]));
      return result;
    } catch (error) { return null; }
  }
  function keyAttribute(property, name, key) {
    try { return value(property[name](key)); }
    catch (error) { return null; }
  }
  function propertyTree(property, path, depth) {
    if (++counts.properties > 100000 || depth > 32) throw new Error("Property capture limit exceeded");
    var node = fields(property, ["name", "matchName", "propertyIndex", "enabled", "active",
      "isEffect", "isMask", "isModified"]);
    node.propertyType = String(property.propertyType);
    node.path = path;
    if (property.isMask) node.maskAttributes = fields(property, ["maskMode", "inverted",
      "maskMotionBlur", "rotoBezier", "color", "locked"]);
    if (property.propertyType !== PropertyType.PROPERTY) {
      node.children = [];
      for (var p = 1; p <= property.numProperties; p++) {
        var child = property.property(p);
        node.children.push(propertyTree(child, path.concat([child.matchName]), depth + 1));
      }
      return node;
    }
    node.propertyValueType = String(property.propertyValueType);
    node.attributes = fields(property, ["canVaryOverTime", "canSetExpression", "expression",
      "expressionEnabled", "expressionError", "isSpatial", "dimensionsSeparated",
      "isSeparationLeader", "isSeparationFollower", "separationDimension", "unitsText",
      "hasMin", "hasMax", "minValue", "maxValue"]);
    node.valueState = "available";
    if (property.propertyValueType === PropertyValueType.NO_VALUE) {
      node.valueState = "no_value";
      node.authoredValue = null;
    } else {
      try { node.authoredValue = value(property.valueAtTime(0, true), property.matchName); }
      catch (error) {
        node.valueState = "unavailable";
        node.authoredValue = null;
        errors.push({ path: path, detail: String(error) });
      }
    }
    node.keys = [];
    for (var k = 1; k <= property.numKeys; k++) {
      if (++counts.keys > 250000) throw new Error("Key capture limit exceeded");
      node.keys.push({
        time: property.keyTime(k), value: value(property.keyValue(k), property.matchName),
        inInterpolation: keyAttribute(property, "keyInInterpolationType", k),
        outInterpolation: keyAttribute(property, "keyOutInterpolationType", k),
        inEase: ease(property, k, true), outEase: ease(property, k, false),
        temporalContinuous: keyAttribute(property, "keyTemporalContinuous", k),
        temporalAutoBezier: keyAttribute(property, "keyTemporalAutoBezier", k),
        inSpatialTangent: keyAttribute(property, "keyInSpatialTangent", k),
        outSpatialTangent: keyAttribute(property, "keyOutSpatialTangent", k),
        spatialContinuous: keyAttribute(property, "keySpatialContinuous", k),
        spatialAutoBezier: keyAttribute(property, "keySpatialAutoBezier", k),
        roving: keyAttribute(property, "keyRoving", k)
      });
    }
    return node;
  }
  function layerSnapshot(layer) {
    if (++counts.layers > 20000) throw new Error("Layer capture limit exceeded");
    var result = fields(layer, ["id", "index", "name", "matchName", "startTime", "inPoint",
      "outPoint", "stretch", "enabled", "solo", "shy", "locked", "label", "threeDLayer",
      "threeDPerChar", "adjustmentLayer", "nullLayer", "guideLayer", "blendingMode",
      "preserveTransparency", "collapseTransformation", "motionBlur", "frameBlending",
      "frameBlendingType", "effectsActive", "hasAudio", "hasVideo", "audioEnabled",
      "timeRemapEnabled", "trackMatteType", "isTrackMatte", "hasTrackMatte", "quality",
      "samplingQuality", "autoOrient"]);
    result.parentId = layer.parent ? layer.parent.id : null;
    result.sourceId = read(layer, "source") ? layer.source.id : null;
    result.trackMatteLayerId = read(layer, "trackMatteLayer") ? layer.trackMatteLayer.id : null;
    try { result.sourceRect = layer.sourceRectAtTime(0, false); }
    catch (error) { result.sourceRect = null; }
    result.properties = [];
    for (var p = 1; p <= layer.numProperties; p++) {
      var property = layer.property(p);
      result.properties.push(propertyTree(property, [property.matchName], 0));
    }
    return result;
  }
  function quote(text) {
    return '"' + String(text).replace(/["\\\u0000-\u001f]/g, function (character) {
      var code = character.charCodeAt(0).toString(16);
      return "\\u" + ("0000" + code).slice(-4);
    }) + '"';
  }
  function json(data) {
    if (data === null || typeof data === "undefined") return "null";
    if (typeof data === "string") return quote(data);
    if (typeof data === "boolean") return data ? "true" : "false";
    if (typeof data === "number") return isFinite(data) ? String(data) : "null";
    if (data instanceof Array) {
      var entries = [];
      for (var i = 0; i < data.length; i++) entries.push(json(data[i]));
      return "[" + entries.join(",") + "]";
    }
    var members = [];
    for (var name in data) if (data.hasOwnProperty(name)) members.push(quote(name) + ":" + json(data[name]));
    return "{" + members.join(",") + "}";
  }
  var snapshot = {
    format: "libre-effects-ae-reference", version: 1,
    application: { version: app.version, buildNumber: app.buildNumber },
    project: { sourcePath: app.project.file.fsName, bitsPerChannel: app.project.bitsPerChannel,
      workingSpace: app.project.workingSpace, linearBlending: app.project.linearBlending,
      linearizeWorkingSpace: app.project.linearizeWorkingSpace },
    enums: {
      interpolation: enumeration(KeyframeInterpolationType, ["LINEAR", "BEZIER", "HOLD"]),
      matte: enumeration(TrackMatteType, ["NO_TRACK_MATTE", "ALPHA", "ALPHA_INVERTED", "LUMA", "LUMA_INVERTED"]),
      mask: enumeration(MaskMode, ["NONE", "ADD", "SUBTRACT", "INTERSECT", "LIGHTEN", "DARKEN", "DIFFERENCE"]),
      paragraph: enumeration(ParagraphJustification, ["LEFT_JUSTIFY", "CENTER_JUSTIFY", "RIGHT_JUSTIFY",
        "FULL_JUSTIFY_LASTLINE_LEFT", "FULL_JUSTIFY_LASTLINE_CENTER", "FULL_JUSTIFY_LASTLINE_RIGHT", "FULL_JUSTIFY_LASTLINE_FULL"]),
      blending: enumeration(BlendingMode, ["NORMAL", "ADD", "MULTIPLY", "SCREEN", "OVERLAY", "DARKEN", "LIGHTEN", "DIFFERENCE"])
    },
    items: [], counts: counts, errors: errors
  };
  for (var i = 1; i <= app.project.numItems; i++) {
    if (++counts.items > 4096) throw new Error("Item capture limit exceeded");
    var item = app.project.item(i);
    var record = fields(item, ["id", "name", "typeName", "width", "height", "pixelAspect",
      "frameRate", "frameDuration", "duration", "hasAudio", "hasVideo", "footageMissing", "useProxy"]);
    record.parentFolderId = item.parentFolder ? item.parentFolder.id : null;
    if (item instanceof CompItem) {
      record.kind = "composition";
      record.settings = fields(item, ["displayStartTime", "displayStartFrame", "bgColor",
        "renderer", "preserveNestedFrameRate", "preserveNestedResolution", "motionBlur",
        "shutterAngle", "shutterPhase", "workAreaStart", "workAreaDuration"]);
      record.layers = [];
      for (var l = 1; l <= item.numLayers; l++) record.layers.push(layerSnapshot(item.layer(l)));
    } else if (item instanceof FootageItem) {
      record.kind = "footage";
      record.file = item.file ? item.file.fsName : null;
      record.source = fields(item.mainSource, ["isStill", "color", "alphaMode", "premulColor",
        "invertAlpha", "conformFrameRate", "nativeFrameRate", "loop", "missingFootagePath",
        "fieldSeparationType", "highQualityFieldSeparation", "removePulldown"]);
      // Do not classify this host object with instanceof. In the observed AE
      // runtime that test labels both real file and solid sources as placeholders.
      // Retain the exposed file/color/interpretation data without inventing a kind.
    } else record.kind = "folder";
    if ((item instanceof CompItem || item instanceof FootageItem) && read(item, "proxySource")) {
      record.proxy = fields(item.proxySource, ["isStill", "alphaMode", "conformFrameRate",
        "nativeFrameRate", "loop", "missingFootagePath"]);
      record.proxy.file = read(item.proxySource, "file") ? item.proxySource.file.fsName : null;
    } else record.proxy = null;
    snapshot.items.push(record);
  }
  destination.encoding = "UTF-8";
  if (!destination.open("w")) throw new Error("Cannot write reference snapshot: " + destination.error);
  try { if (!destination.write(json(snapshot))) throw new Error("Reference write failed"); }
  finally { destination.close(); }
}());
