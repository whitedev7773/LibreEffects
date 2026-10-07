// This file is the private host implementation, not a copied AE expression.
// Direct eval is intentional here: AE expressions return JavaScript statement
// completion values (including if/else), not merely a final textual expression.
(function () {
  "use strict";
  return function (input) {
    const data = JSON.parse(input);
    // These originals are private and used only with small host-owned arrays.
    const arraySlice = Array.prototype.slice;
    const arrayJoin = Array.prototype.join;
    const arraySort = Array.prototype.sort;
    const programs = new Map();
    // Only this map brands path values. Guest objects, symbols and coercion
    // hooks can never manufacture a path result or expose its private arrays.
    const pathValues = new WeakMap();
    const cache = new Map();
    const dependencies = new Map();
    const active = [];
    const layerViews = new Map();
    let failure = null;
    let hostReads = 0;
    let expressionEvaluations = 0;

    function reject(kind, message) {
      if (failure === null) {
        failure = { kind, message: String(message).slice(0, 2048), property: active.length ? active[active.length - 1] : null };
      }
      throw new Error(message);
    }
    function read() {
      hostReads += 1;
      if (hostReads > data.max_host_reads) {
        reject("budget", "Expression host-read budget exceeded");
      }
    }
    function mutation() {
      return reject("unsupported", "Expression host values are read-only");
    }
    function readonly(target, label) {
      for (const key of Reflect.ownKeys(target)) {
        const descriptor = Object.getOwnPropertyDescriptor(target, key);
        for (const item of [descriptor.value, descriptor.get, descriptor.set]) {
          if (typeof item === "function") {
            if (item.prototype) { Object.freeze(item.prototype); }
            Object.freeze(item);
          }
        }
      }
      if (typeof target === "function" && target.prototype) { Object.freeze(target.prototype); }
      Object.freeze(target);
      return new Proxy(target, {
        get(object, property, receiver) {
          if (!Reflect.has(object, property)) {
            return reject("unsupported", label + "." + String(property) + " is unsupported");
          }
          return Reflect.get(object, property, receiver);
        },
        set: mutation,
        defineProperty: mutation,
        deleteProperty: mutation,
        setPrototypeOf: mutation,
        preventExtensions: mutation,
      });
    }
    function vector(value) {
      return Array.isArray(value) ? readonly(arraySlice.call(value), "Property value") : value;
    }
    function record(fields, label) {
      return readonly(Object.assign(Object.create(null), fields), label);
    }
    function finite(value, label) {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        reject("invalid_result", label + " must be a finite number");
      }
      return value;
    }
    function checkedText(value) {
      if (typeof value !== "string" || value.length > 16384) {
        reject("invalid_result", "Source Text requires a string of at most 16384 UTF-8 bytes");
      }
      let bytes = 0;
      for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        if (code >= 0xD800 && code <= 0xDBFF) {
          const next = value.charCodeAt(++index);
          if (!(next >= 0xDC00 && next <= 0xDFFF)) {
            reject("invalid_result", "Source Text contains an unpaired UTF-16 surrogate");
          }
          bytes += 4;
        } else {
          if (code >= 0xDC00 && code <= 0xDFFF) {
            reject("invalid_result", "Source Text contains an unpaired UTF-16 surrogate");
          }
          bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : 3;
        }
        if (bytes > 16384) {
          reject("invalid_result", "Source Text exceeds 16384 UTF-8 bytes");
        }
      }
      return value;
    }
    // Bounded copies use own data descriptors, never guest array getters or
    // iterators. Proxy descriptor traps may execute but cannot change a copied
    // component; normal VM budgets and the sticky failure guard remain active.
    function dataItem(array, key, label) {
      const descriptor = Object.getOwnPropertyDescriptor(array, key);
      if (!descriptor || !("value" in descriptor)) {
        reject("invalid_result", label + " requires dense arrays with data elements");
      }
      return descriptor.value;
    }
    function arrayLength(value, max, label) {
      if (!Array.isArray(value)) {
        reject("invalid_result", label + " requires an array");
      }
      const length = dataItem(value, "length", label);
      if (!Number.isInteger(length) || length < 0 || length > max) {
        reject("invalid_result", label + " exceeds its array bound");
      }
      return length;
    }
    function coordinateList(value, expected, allowEmpty, label) {
      const length = arrayLength(value, 1024, label);
      if (length !== expected && !(allowEmpty && length === 0)) {
        reject("invalid_result", label + " must match the path vertex count");
      }
      const result = [];
      for (let index = 0; index < expected; index += 1) {
        const point = length === 0 ? [0, 0] : dataItem(value, String(index), label);
        if (arrayLength(point, 2, label) !== 2) {
          reject("invalid_result", label + " requires two-component points");
        }
        const copy = [];
        for (let axis = 0; axis < 2; axis += 1) {
          const component = finite(dataItem(point, String(axis), label), label);
          if (Math.abs(component) > 1000000) {
            reject("invalid_result", "Path coordinates must be within +/-1000000");
          }
          copy.push(component);
        }
        result.push(Object.freeze(copy));
      }
      return Object.freeze(result);
    }
    function brandPath(path) {
      const view = record({}, "Path");
      pathValues.set(view, Object.freeze(path));
      return view;
    }
    function createPath(points, inTangents, outTangents, isClosed) {
      read();
      if (typeof isClosed !== "boolean") {
        reject("invalid_result", "createPath requires an explicit boolean closed flag");
      }
      const count = arrayLength(points, 1024, "Path vertices");
      if (count < (isClosed ? 3 : 2)) {
        reject("invalid_result", "Paths require at least three closed or two open vertices");
      }
      const vertices = coordinateList(points, count, false, "Path vertices");
      const incoming = coordinateList(inTangents, count, true, "Incoming path handles");
      const outgoing = coordinateList(outTangents, count, true, "Outgoing path handles");
      if (failure !== null) { throw new Error(failure.message); }
      return brandPath({ vertices, in_tangents: incoming, out_tangents: outgoing, closed: isClosed });
    }
    function linear(t, tMin, tMax, first, last) {
      read();
      finite(t, "Interpolation time");
      finite(tMin, "Interpolation start");
      finite(tMax, "Interpolation end");
      if (tMax <= tMin) {
        reject("invalid_result", "linear requires an increasing finite time range");
      }
      const span = finite(tMax - tMin, "Interpolation time range");
      const amount = t <= tMin ? 0 : t >= tMax ? 1 : (t - tMin) / span;
      const interpolate = (a, b) => {
        finite(a, "Interpolation start value");
        finite(b, "Interpolation end value");
        return finite(a * (1 - amount) + b * amount, "Interpolated value");
      };
      if (typeof first === "number" && typeof last === "number") {
        return interpolate(first, last);
      }
      const length = arrayLength(first, 3, "Interpolation start value");
      if (length < 2 || arrayLength(last, 3, "Interpolation end value") !== length) {
        reject("invalid_result", "linear endpoints require matching two- or three-component vectors");
      }
      const result = [];
      for (let index = 0; index < length; index += 1) {
        result.push(interpolate(dataItem(first, String(index), "Interpolation start value"),
          dataItem(last, String(index), "Interpolation end value")));
      }
      if (failure !== null) { throw new Error(failure.message); }
      return vector(result);
    }
    function authoredValue(value) {
      if (value !== null && typeof value === "object" && !Array.isArray(value)) {
        // This object came from the Rust-validated snapshot, never guest code.
        return brandPath({
          vertices: coordinateList(value.vertices, value.vertices.length, false, "Path vertices"),
          in_tangents: coordinateList(value.in_tangents, value.vertices.length, false, "Incoming path handles"),
          out_tangents: coordinateList(value.out_tangents, value.vertices.length, false, "Outgoing path handles"),
          closed: value.closed,
        });
      }
      return vector(value);
    }
    function framesToTime(frames, fps) {
      read();
      finite(frames, "Frame count");
      if (fps === undefined) {
        return finite(frames * data.frame_rate.denominator / data.frame_rate.numerator, "Converted time");
      }
      finite(fps, "Frame rate");
      if (fps <= 0) {
        reject("invalid_result", "Frame rate must be positive");
      }
      return finite(frames / fps, "Converted time");
    }
    function markerView(layer) {
      return record({
        numKeys: layer.markers.length,
        key(index) {
          read();
          if (!Number.isInteger(index) || index < 1 || index > layer.markers.length) {
            reject("missing_reference", "Marker key requires a valid one-based index");
          }
          const marker = layer.markers[index - 1];
          return record({ time: marker.time, comment: marker.comment, index }, "Marker key");
        },
      }, "Marker");
    }
    function layerView(index) {
      if (layerViews.has(index)) {
        return layerViews.get(index);
      }
      const layer = data.layers[index];
      const transform = Object.create(null);
      for (const property of ["position", "scale", "opacity"]) {
        Object.defineProperty(transform, property, { enumerable: true, get() { return sample(layer[property]); } });
      }
      const transformView = readonly(transform, "Transform");
      const marker = markerView(layer);
      const view = Object.create(null);
      Object.assign(view, {
        name: layer.name,
        index: index + 1,
        startTime: layer.start_time,
        inPoint: layer.in_point,
        outPoint: layer.out_point,
        transform: transformView,
        marker,
        effect(name) {
          read();
          if (typeof name !== "string") {
            reject("unsupported", "Effect lookup requires an exact effect name");
          }
          const slider = layer.sliders.find(entry => entry[0] === name);
          if (!slider) {
            reject("missing_reference", "Missing slider effect: " + name);
          }
          return readonly(function (property) {
            read();
            if (property !== 1 && property !== "Slider" && property !== "ADBE Slider Control-0001") {
              reject("unsupported", "Slider controls expose only property 1 (Slider)");
            }
            return sample(slider[1]);
          }, "Slider effect");
        },
      });
      for (const property of ["position", "scale", "opacity"]) {
        Object.defineProperty(view, property, { enumerable: true, get() { return sample(layer[property]); } });
      }
      if (layer.source_text !== null) {
        const text = Object.create(null);
        Object.defineProperty(text, "sourceText", { enumerable: true, get() { return sample(layer.source_text); } });
        Object.defineProperty(view, "text", { enumerable: true, value: readonly(text, "Text") });
      }
      const result = readonly(view, "Layer");
      layerViews.set(index, result);
      return result;
    }
    const comp = record({
      width: data.width,
      height: data.height,
      duration: data.duration,
      time: data.time,
      frameDuration: data.frame_rate.denominator / data.frame_rate.numerator,
      frameRate: data.frame_rate.numerator / data.frame_rate.denominator,
      numLayers: data.layers.length,
      layer(key) {
        read();
        const index = typeof key === "string"
          ? data.layers.findIndex(layer => layer.name === key)
          : (Number.isInteger(key) ? key - 1 : -1);
        if (index < 0 || index >= data.layers.length) {
          reject("missing_reference", "Missing layer: " + String(key));
        }
        return layerView(index);
      },
    }, "Composition");

    function checkedValue(value, authored) {
      if (typeof authored === "number") {
        return finite(value, "Expression result");
      }
      if (typeof authored === "string") { return checkedText(value); }
      if (!Array.isArray(authored)) {
        if (!pathValues.has(value)) {
          reject("invalid_result", "Path expression results must come from createPath or their authored value");
        }
        return value;
      }
      if (!Array.isArray(value) || value.length !== authored.length) {
        reject("invalid_result", "Expression result must match the authored vector dimensions");
      }
      // Copy immediately. Guest-owned arrays/getters never escape the VM.
      const result = [];
      for (let index = 0; index < authored.length; index += 1) {
        result.push(finite(value[index], "Expression vector component"));
      }
      return vector(result);
    }
    function sample(index) {
      read();
      if (active.length) {
        dependencies.get(active[active.length - 1]).add(index);
      }
      if (cache.has(index)) {
        return cache.get(index);
      }
      if (active.includes(index)) {
        const path = [...active, index].map(item => {
          const property = data.properties[item];
          return data.layers[property.layer].name + "." + property.name;
        });
        reject("cycle", "Expression dependency cycle: " + arrayJoin.call(path, " -> "));
      }
      if (active.length >= data.max_dependency_depth) {
        reject("budget", "Expression dependency-depth budget exceeded");
      }
      const property = data.properties[index];
      dependencies.set(index, new Set());
      active.push(index);
      try {
        let result = authoredValue(property.value);
        if (property.source_id !== null) {
          expressionEvaluations += 1;
          if (expressionEvaluations > data.max_expression_evaluations) {
            reject("budget", "Expression evaluation-count budget exceeded");
          }
          const layer = layerView(property.layer);
          // Reuse only the immutable wrapper for this batch's exact source ID.
          // Every call still gets fresh parameters and a fresh lexical scope;
          // direct eval still parses the source and preserves JS completion values.
          const locals = property.local_bindings;
          // Rust validates ASCII syntax/reserved names. Check actual VM globals
          // as well, without invoking the getters for forbidden capabilities.
          for (const name of locals) {
            if (Reflect.has(globalThis, name)) {
              reject("invalid_snapshot", "Expression local binding shadows a global capability");
            }
          }
          const programKey = JSON.stringify([property.source_id, locals]);
          let program = programs.get(programKey);
          if (program === undefined) {
            const declarations = locals.length ? 'let ' + arrayJoin.call(locals, ',') + ';\n' : '';
            program = new Function("thisComp", "thisLayer", "time", "inPoint", "outPoint", "startTime", "value", "framesToTime", "transform", "marker", "effect", "linear", "createPath",
              '"use strict";\n' + declarations + 'return eval(' + JSON.stringify(data.sources[property.source_id]) + ');');
            Object.freeze(program.prototype);
            Object.freeze(program);
            programs.set(programKey, program);
          }
          result = program(comp, layer, data.time, layer.inPoint, layer.outPoint, layer.startTime,
            result, framesToTime, layer.transform, layer.marker, layer.effect, linear, createPath);
        }
        if (failure !== null) {
          throw new Error(failure.message);
        }
        result = checkedValue(result, property.value);
        if (failure !== null) { throw new Error(failure.message); }
        cache.set(index, result);
        return result;
      } catch (error) {
        if (failure === null) {
          reject("java_script", (String(error) + (error && typeof error.stack === "string" ? "\n" + error.stack : "")).slice(0, 2048));
        }
        throw error;
      } finally {
        active.pop();
      }
    }

    // Deterministic, synchronous language only. No native host bindings exist.
    // Freeze intrinsics so one expression cannot corrupt another's execution,
    // validators, dependency cache, or output serialization.
    for (const name of ["Date", "Promise", "WeakRef", "FinalizationRegistry", "Atomics", "SharedArrayBuffer",
      "app", "File", "Folder", "Socket", "system", "$", "Window", "alert", "confirm", "fetch", "XMLHttpRequest", "setTimeout", "setInterval"]) {
      Object.defineProperty(globalThis, name, {
        configurable: false,
        get() { return reject("unsupported", name + " is unavailable in read-only expressions"); },
      });
    }
    Object.defineProperty(Math, "random", {
      value() { return reject("unsupported", "Math.random is unavailable; deterministic random helpers are not implemented"); },
    });
    // Pinned QuickJS has native sparse-array loops that do not poll its
    // interrupt hook. These nonessential operations must not be reachable by
    // guest code; a bytecode timeout alone cannot bound them. Do not replace
    // this with a receiver.length precheck (Proxy/getter TOCTOU bypasses it).
    function exclude(object, name, label) {
      if (Object.hasOwn(object, name)) {
        Object.defineProperty(object, name, {
          value() { return reject("unsupported", label + "." + name + " is outside the bounded expression subset"); },
        });
      }
    }
    for (const name of ["concat", "join", "toLocaleString", "shift", "unshift", "reverse", "sort", "slice",
      "splice", "copyWithin", "flat", "flatMap", "fill", "toReversed", "toSorted", "toSpliced", "with"]) {
      exclude(Array.prototype, name, "Array");
    }
    if (typeof Iterator !== "undefined") {
      for (const name of ["chunks", "drop", "filter", "flatMap", "map", "take", "windows", "every", "find",
        "forEach", "some", "includes", "join", "reduce", "toArray"]) {
        exclude(Iterator.prototype, name, "Iterator");
      }
      for (const name of ["concat", "zip", "zipKeyed"]) { exclude(Iterator, name, "Iterator"); }
    }
    const visited = new Set();
    function freezeIntrinsics(object) {
      if (object === null || (typeof object !== "object" && typeof object !== "function") || visited.has(object)) {
        return;
      }
      visited.add(object);
      freezeIntrinsics(Object.getPrototypeOf(object));
      for (const key of Reflect.ownKeys(object)) {
        const descriptor = Object.getOwnPropertyDescriptor(object, key);
        if ("value" in descriptor) {
          freezeIntrinsics(descriptor.value);
        }
        freezeIntrinsics(descriptor.get);
        freezeIntrinsics(descriptor.set);
      }
      Object.freeze(object);
    }
    // Iterator prototypes are not reachable through constructor properties.
    // The host itself iterates arrays/maps/sets after guest execution.
    for (const iterator of [
      [][Symbol.iterator](), new Map().entries(), new Set().values(),
      ""[Symbol.iterator](), "".matchAll(/./g), (function* () {})(),
      async function () {}, (async function* () {})(),
    ]) {
      freezeIntrinsics(Object.getPrototypeOf(iterator));
    }
    if (typeof Iterator !== "undefined") {
      freezeIntrinsics(Object.getPrototypeOf(Iterator.from({ next() { return { done: true }; } })));
    }
    freezeIntrinsics(framesToTime);
    freezeIntrinsics(linear);
    freezeIntrinsics(createPath);
    freezeIntrinsics(globalThis);

    try {
      for (const index of data.requested) {
        sample(index);
      }
      if (failure !== null) { throw new Error(failure.message); }
      return JSON.stringify({
        error: null,
        values: Array.from(cache, ([property, value]) => ({
          property,
          value: pathValues.has(value) ? pathValues.get(value) : Array.isArray(value) ? Array.from(value) : value,
          dependencies: arraySort.call(Array.from(dependencies.get(property)), (a, b) => a - b),
        })),
        expression_evaluations: expressionEvaluations,
        host_reads: hostReads,
      });
    } catch (error) {
      return JSON.stringify({
        error: failure || { kind: "java_script", message: String(error).slice(0, 2048), property: null },
        values: [],
        expression_evaluations: expressionEvaluations,
        host_reads: hostReads,
      });
    }
  };
})()
