/* Bounded synchronous AE/ScriptUI compatibility facade. This is a normal JS
 * program running once in an isolated QuickJS VM. UI callbacks keep their real
 * closures; no parser substitution, command guessing or script replay occurs. */
(function (global, nativeHost, nativeUI, nativeFail, nativePrint) {
  'use strict';
  const parse = JSON.parse.bind(JSON), stringify = JSON.stringify.bind(JSON);
  const own = Object.prototype.hasOwnProperty;
  function fail(message) { message = String(message); nativeFail(message); throw new Error(message); }
  function result(json) { const r = parse(json); if (!r.ok) throw new Error(r.error); return r.value; }
  function host(op, args) { return result(nativeHost(op, stringify(args || {}))); }
  function ui(request) { return result(nativeUI(stringify(request))); }
  function unsupported(name) { return fail('Unsupported host feature: ' + name); }
  function readonly(name) { return fail('Read-only host property: ' + name); }
  function boundedText(value, max) { value = String(value); if (value.length > max) fail('ScriptUI text exceeds its limit'); return value; }
  function strict(target, name) {
    return new Proxy(target, {
      get(object, key, receiver) {
        if (typeof key === 'symbol' || key === 'then' || key === 'toJSON') return Reflect.get(object, key, receiver);
        if (!(key in object)) return unsupported(name + '.' + key);
        return Reflect.get(object, key, receiver);
      },
      set(object, key, value) {
        if (!(key in object)) return unsupported(name + '.' + String(key));
        let at = object, descriptor;
        while (at && !(descriptor = Object.getOwnPropertyDescriptor(at, key))) at = Object.getPrototypeOf(at);
        if (descriptor && !descriptor.set && !descriptor.writable) return readonly(name + '.' + String(key));
        return Reflect.set(object, key, value, object);
      },
      defineProperty() { return unsupported(name + '.defineProperty'); },
      deleteProperty() { return unsupported(name + '.deleteProperty'); },
      setPrototypeOf() { return unsupported(name + '.setPrototypeOf'); }
    });
  }
  function getter(object, name, get, set) { Object.defineProperty(object, name, { enumerable: true, get, set }); }
  function snapshot() { return host('snapshot'); }
  function compositionData(id) { const c = snapshot().items.find(x => x.type === 'composition' && x.id === id); if (!c) fail('Composition reference is stale'); return c; }
  function layerData(comp, id) { return host('layer_get', {comp, id}); }
  const compositions = new Map(), layers = new Map();
  function CompItem() { unsupported('construct CompItem'); }
  function FootageItem() { unsupported('construct FootageItem'); }
  function FolderItem() { unsupported('construct FolderItem'); }
  function Layer() { unsupported('construct Layer'); }
  function AVLayer() { unsupported('construct AVLayer'); }
  AVLayer.prototype = Layer.prototype;
  function TextLayer() { unsupported('construct TextLayer'); }
  TextLayer.prototype = Object.create(Layer.prototype);
  function collection(getItems, resolve, label) {
    const target = {};
    getter(target, 'length', () => getItems().length);
    target.item = function (index) { return indexed(index); };
    function indexed(index) {
      if (!Number.isInteger(index) || index < 1) fail(label + ' uses 1-based integer indices');
      const items = getItems();
      if (index > items.length) fail(label + ' index is out of range');
      return resolve(items[index - 1]);
    }
    return new Proxy(strict(target, label), {
      get(object, key, receiver) { if (typeof key === 'string' && /^\d+$/.test(key)) return indexed(Number(key)); return Reflect.get(object,key,receiver); },
      set() { return readonly(label); }
    });
  }
  function compRef(id) {
    if (compositions.has(id)) return compositions.get(id);
    const comp = Object.create(CompItem.prototype);
    getter(comp, 'id', () => id);
    for (const key of ['name', 'width', 'height', 'frameRate', 'frameDuration', 'duration', 'time']) getter(comp, key, () => { const value = compositionData(id)[key]; if (key === 'time' && value === null) return unsupported('Inactive CompItem.time (no playhead context)'); return value; }, () => readonly('CompItem.' + key));
    getter(comp, 'numLayers', () => compositionData(id).layers.length);
    getter(comp, 'selectedLayers', () => compositionData(id).layers.filter(x => x.selected).map(x => layerRef(id,x.id)));
    getter(comp, 'layers', () => collection(() => compositionData(id).layers, x => layerRef(id,x.id), 'LayerCollection'));
    comp.layer = function (index) {
      const list = compositionData(id).layers;
      if (typeof index === 'string') { const layer = list.find(x => x.name === index); return layer ? layerRef(id,layer.id) : null; }
      if (!Number.isInteger(index) || index < 1 || index > list.length) fail('CompItem.layer uses an in-range 1-based index');
      return layerRef(id,list[index-1].id);
    };
    const proxy = strict(comp,'CompItem'); compositions.set(id,proxy); return proxy;
  }
  function layerRef(comp,id) {
    const identity = comp + ':' + id;
    if (layers.has(identity)) return layers.get(identity);
    const data = layerData(comp,id), layer = Object.create(data.hasText ? TextLayer.prototype : Layer.prototype);
    getter(layer, 'id', () => id);
    getter(layer, 'containingComp', () => compRef(comp));
    for (const key of ['name','enabled','locked','inPoint','outPoint','index','selected','startTime','label']) getter(layer,key,() => {
      const data = layerData(comp,id);
      if (!own.call(data,key)) return unsupported('Layer.' + key);
      return data[key];
    }, value => host('layer_set',{comp,id,field:key,value}));
    getter(layer,'threeDLayer',() => layerData(comp,id).threeDLayer,value => host('layer_set',{comp,id,field:'threeDLayer',value}));
    layer.duplicate = function () { const data = host('layer_duplicate',{comp,id}); return layerRef(comp,typeof data === 'number' ? data : data.id); };
    layer.remove = function () { return host('layer_remove',{comp,id}); };
    layer.moveToBeginning = function () { return host('layer_move_to_beginning',{comp,id}); };
    layer.property = function (name) {
      if (name === 'ADBE Transform Group' || name === 'Transform') return propertyGroup(comp,id,'transform');
      if (name === 'ADBE Text Properties' || name === 'Text') return layerData(comp,id).hasText ? propertyGroup(comp,id,'text') : null;
      if (name === 'Marker' || name === 'ADBE Marker') return propertyRef(comp,id,'marker');
      if (name === 'Source Text' || name === 'ADBE Text Document') return layerData(comp,id).hasText ? propertyRef(comp,id,'sourceText') : null;
      return unsupported('Layer.property(' + name + ')');
    };
    getter(layer,'transform',() => propertyGroup(comp,id,'transform'));
    const proxy = strict(layer,'Layer'); layers.set(identity,proxy); return proxy;
  }
  function propertyGroup(comp,id,kind) {
    const group = { property(name) {
      const map = kind === 'transform' ? {'ADBE Position':'position',Position:'position','ADBE Scale':'scale',Scale:'scale','ADBE Opacity':'opacity',Opacity:'opacity'} : {'ADBE Text Document':'sourceText','Source Text':'sourceText'};
      if (!own.call(map,name)) return unsupported('PropertyGroup.property(' + name + ')');
      return propertyRef(comp,id,map[name]);
    }};
    if (kind === 'transform') { getter(group,'position',() => propertyRef(comp,id,'position')); getter(group,'opacity',() => propertyRef(comp,id,'opacity')); getter(group,'scale',() => propertyRef(comp,id,'scale')); }
    return strict(group,'PropertyGroup');
  }
  function TextDocument(text) {
    this.text = String(text === undefined ? '' : text);
    return strict(this,'TextDocument');
  }
  function MarkerValue(comment) {
    this.comment = String(comment === undefined ? '' : comment);
    this.duration = 0;
    return strict(this,'MarkerValue');
  }
  function KeyframeEase(speed,influence) {
    if (!Number.isFinite(speed) || !Number.isFinite(influence) || influence < 0.1 || influence > 100) fail('Invalid KeyframeEase');
    this.speed = speed; this.influence = influence;
    return strict(this,'KeyframeEase');
  }
  function propertyRef(comp,id,property) {
    const base = {comp,id,property}, prop = {};
    function get(field) { return host('property_get',Object.assign({},base,{field})); }
    function key(action,index,args) { return host('property_key',Object.assign({},base,args,{action,index})); }
    function value(raw) { return property === 'sourceText' ? new TextDocument(raw.text) : property === 'marker' && raw ? Object.assign(new MarkerValue(raw.comment),{duration:raw.duration || 0}) : raw; }
    getter(prop,'value',() => { if (property === 'marker') return unsupported('Marker.value (use keyValue)'); return value(get('value').value); });
    function expressionInfo() { if (!['position','scale','opacity'].includes(property)) return unsupported('Expression target '+property); return get('metadata'); }
    getter(prop,'expression',() => expressionInfo().expression, value => host('property_expression',Object.assign({},base,{field:'source',value})));
    getter(prop,'expressionEnabled',() => expressionInfo().expressionEnabled, value => host('property_expression',Object.assign({},base,{field:'enabled',value})));
    getter(prop,'numKeys',() => get('metadata').numKeys);
    getter(prop,'propertyValueType',() => get('metadata').propertyValueType);
    getter(prop,'dimensionsSeparated',() => get('metadata').dimensionsSeparated,value => { if(value !== false) return unsupported('Property.dimensionsSeparated'); return undefined; });
    prop.setValue = function (value) { return host('property_set',Object.assign({},base,{value})); };
    prop.setValueAtTime = function (time,value) { return host(property === 'marker' ? 'marker_set' : 'property_set',Object.assign({},base,{time,value})); };
    prop.keyTime = function (index) { return key('get_time',index); };
    prop.keyValue = function (index) { return value(key('get_value',index)); };
    function keyMetadata(index,field) {
      const metadata = host('property_key_metadata',Object.assign({},base,{index}));
      if (!own.call(metadata,field)) return unsupported('Property key metadata '+field);
      return metadata[field];
    }
    prop.keyInInterpolationType = function (index) { return keyMetadata(index,'in_interpolation').toUpperCase(); };
    prop.keyOutInterpolationType = function (index) { return keyMetadata(index,'out_interpolation').toUpperCase(); };
    prop.keyInTemporalEase = function (index) { const ease = keyMetadata(index,'in_ease'); return [new KeyframeEase(ease.speed,ease.influence)]; };
    prop.keyOutTemporalEase = function (index) { const ease = keyMetadata(index,'out_ease'); return [new KeyframeEase(ease.speed,ease.influence)]; };
    prop.keyTemporalContinuous = function (index) { return keyMetadata(index,'temporal_continuous'); };
    prop.keyTemporalAutoBezier = function (index) { return keyMetadata(index,'temporal_auto_bezier'); };
    prop.keyInSpatialTangent = function (index) { return keyMetadata(index,'in_tangent'); };
    prop.keyOutSpatialTangent = function (index) { return keyMetadata(index,'out_tangent'); };
    prop.keySpatialContinuous = function (index) { return keyMetadata(index,'spatial_continuous'); };
    prop.keySpatialAutoBezier = function (index) { return keyMetadata(index,'spatial_auto_bezier'); };
    prop.nearestKeyIndex = function (time) { return key('nearest',undefined,{time}); };
    prop.removeKey = function (index) { return key('remove',index); };
    prop.setInterpolationTypeAtKey = function (index,inType,outType) { return key('interpolation',index,{inType,outType:outType === undefined ? inType : outType}); };
    prop.setTemporalEaseAtKey = function (index,inEase,outEase) { return key('ease',index,{inEase,outEase:outEase === undefined ? inEase : outEase}); };
    prop.setTemporalContinuousAtKey = function (index,value) { return key('temporal_continuous',index,{value}); };
    prop.setTemporalAutoBezierAtKey = function (index,value) { return key('temporal_auto_bezier',index,{value}); };
    prop.setSpatialTangentsAtKey = function (index,inTangent,outTangent) { return key('spatial_tangents',index,{inTangent,outTangent}); };
    prop.setSpatialContinuousAtKey = function (index,value) { return key('spatial_continuous',index,{value}); };
    prop.setSpatialAutoBezierAtKey = function (index,value) { return key('spatial_auto_bezier',index,{value}); };
    return strict(prop,'Property');
  }
  const project = {};
  getter(project,'numItems',() => snapshot().items.length);
  const otherItems = new Map();
  function itemRef(item) {
    if (item.type === 'composition') return compRef(item.id);
    if (item.type !== 'footage' && item.type !== 'folder') return unsupported('Project item type ' + item.type);
    const identity = item.type + ':' + item.id;
    if (otherItems.has(identity)) return otherItems.get(identity);
    const object = Object.create(item.type === 'footage' ? FootageItem.prototype : FolderItem.prototype);
    getter(object,'id',() => item.id);
    for (const key of ['name','width','height']) {
      if (!own.call(item,key)) continue;
      getter(object,key,() => {
        const current = snapshot().items.find(x => x.type === item.type && x.id === item.id);
        if (!current) fail('Project item reference is stale');
        return current[key];
      },() => readonly(item.type + '.' + key));
    }
    const proxy = strict(object,item.type === 'footage' ? 'FootageItem' : 'FolderItem');
    otherItems.set(identity,proxy); return proxy;
  }
  getter(project,'items',() => collection(() => snapshot().items,itemRef,'ItemCollection'));
  project.item = function (index) { return project.items.item(index); };
  getter(project,'activeItem',() => { const id = snapshot().activeItem; return id == null ? null : compRef(id); });
  let undoDepth = 0;
  const app = {project:strict(project,'Project'),beginUndoGroup(name) { boundedText(name,1024); if (++undoDepth > 32) fail('Undo group nesting limit exceeded'); },endUndoGroup() { if (undoDepth < 1) fail('No script undo group is open'); undoDepth--; }};

  // ScriptUI's supported controls are session-local metadata plus native events.
  // Layout hints are retained; unsupported controls/resource-string creation fail.
  let nextControlId = 1, controlCount = 0, visibleWindow = null, nextFocusRequest = 0;
  const controls = new Map();
  function font(name,style,size) {
    if (!Number.isFinite(size) || size < 1 || size > 200) fail('ScriptUI font size must be between 1 and 200');
    style = String(style || 'REGULAR').toUpperCase();
    if (!['REGULAR','BOLD','ITALIC','BOLDITALIC'].includes(style)) unsupported('ScriptUI font style ' + style);
    return strict({name:boundedText(name || 'sans-serif',256),style,size},'ScriptUIFont');
  }
  function size(value) { if (value === null || value === undefined) return null; if (!Array.isArray(value) || value.length !== 2 || value.some(x => !Number.isFinite(x) || x < 0 || x > 4096)) fail('ScriptUI sizes must have two values between 0 and 4096'); return value; }
  function Control(kind,text,options,parent) {
    if (++controlCount > 256) fail('ScriptUI control budget exceeded');
    const raw = {id:nextControlId++,type:kind,text:boundedText(text == null ? '' : text,262144),enabled:true,visible:true,active:false,parent:parent || null,children:[],orientation:'column',alignment:null,alignChildren:null,spacing:8,margins:12,minimumSize:null,preferredSize:null,maximumSize:null,helpTip:'',onClick:null,onChanging:null,onShow:null,onResize:null,onResizing:null};
    let active = false, focusRequest = 0;
    delete raw.active;
    getter(raw,'active',() => active,value => {
      active = !!value;
      if (active) {
        if (nextFocusRequest >= Number.MAX_SAFE_INTEGER) fail('ScriptUI focus request limit exceeded');
        focusRequest = ++nextFocusRequest;
      }
    });
    const childList = raw.children;
    delete raw.children; getter(raw,'children',() => childList.slice());
    for (const key of ['id','type','parent']) Object.defineProperty(raw,key,{value:raw[key],enumerable:true,writable:false,configurable:false});
    raw.graphics = strict({font:font('sans-serif','REGULAR',13)},'ScriptUIGraphics');
    raw.multiline = !!(options && options.multiline);
    raw.layout = strict({layout(){},resize(){}},'ScriptUILayout');
    raw.add = function (kind,bounds,text,options) {
      kind = String(kind).toLowerCase();
      if (!['dialog','group','panel'].includes(raw.type)) return unsupported('Control.add on ' + raw.type);
      if (!['group','panel','statictext','edittext','button'].includes(kind)) return unsupported('ScriptUI control ' + kind);
      if (bounds !== undefined && bounds !== null) return unsupported('ScriptUI absolute bounds');
      if (options) for (const key of Object.keys(options)) if (!['multiline','scrolling','wantReturn','name'].includes(key)) return unsupported('ScriptUI creation option ' + key);
      const child = Control(kind,text,options,proxy); childList.push(child);
      if (options && options.name) {
        if (options.name === 'ok') windowOf(proxy).defaultElement = child;
        if (options.name === 'cancel') windowOf(proxy).cancelElement = child;
      }
      return child;
    };
    const proxy = strict(raw,'ScriptUI.' + kind); controls.set(raw.id,{raw,proxy,focusRequest:() => focusRequest}); return proxy;
  }
  function windowOf(control) { while (control.parent) control = control.parent; return control; }
  function node(control) {
    const entry = controls.get(control.id), raw = entry.raw;
    return {id:raw.id,kind:raw.type,text:boundedText(raw.text,262144),enabled:!!raw.enabled,visible:!!raw.visible,active:!!raw.active,focus_request:entry.focusRequest(),multiline:!!raw.multiline,orientation:raw.orientation === 'row' ? 'row' : 'column',children:raw.children.map(node),minimum_size:size(raw.minimumSize),preferred_size:size(raw.preferredSize),help_tip:boundedText(raw.helpTip,4096)};
  }
  function invoke(control,name) {
    const handler = control[name];
    if (handler != null) {
      if (typeof handler !== 'function') fail('ScriptUI callback must be a function');
      try { handler.call(control); }
      catch (error) {
        let message = 'ScriptUI ' + name + ' callback failed';
        try { message += ': ' + String(error); } catch (_) {}
        nativeFail(message); throw error;
      }
    }
  }
  function Window(kind,title,bounds,options) {
    if (kind !== 'dialog') return unsupported('ScriptUI Window type ' + kind);
    if (bounds !== undefined && bounds !== null) return unsupported('ScriptUI absolute window bounds');
    if (options) for (const key of Object.keys(options)) if (key !== 'resizeable' && key !== 'resizable') return unsupported('ScriptUI window option ' + key);
    const win = Control('dialog',title,null,null), raw = controls.get(win.id).raw;
    raw.defaultElement = null; raw.cancelElement = null;
    raw.center = function () {};
    let closed = false, returnCode = 0, shown = false;
    raw.close = function (code) {
      returnCode = code === undefined ? 0 : Number(code);
      if (!Number.isInteger(returnCode)) fail('ScriptUI close code must be an integer');
      closed = true;
      if (returnCode === 0 || returnCode === 2) nativeFail('Script canceled');
    };
    raw.show = function () {
      if (shown) return unsupported('Showing the same ScriptUI window more than once');
      shown = true;
      if (visibleWindow !== null) return unsupported('Nested ScriptUI dialogs');
      visibleWindow = win; closed = false; returnCode = 0;
      try {
        invoke(win,'onShow');
        while (!closed) {
          const defaultElement = raw.defaultElement, cancelElement = raw.cancelElement;
          for (const element of [defaultElement,cancelElement]) if (element && (element.type !== 'button' || windowOf(element) !== win)) fail('ScriptUI default/cancel element must be a button in this dialog');
          const event = ui({kind:'dialog',id:raw.id,title:boundedText(raw.text,1024),root:node(win),default_element:defaultElement ? defaultElement.id : null,cancel_element:cancelElement ? cancelElement.id : null,resizable:!!(options && (options.resizeable || options.resizable))});
          if (event.kind === 'close') { nativeFail('Script canceled'); if (cancelElement) invoke(cancelElement,'onClick'); raw.close(0); }
          else if (event.kind === 'resize') { invoke(win,'onResizing'); invoke(win,'onResize'); }
          else {
            const control = controls.get(event.control_id).proxy;
            if (windowOf(control) !== win) fail('Stale ScriptUI control');
            if (event.kind === 'change') { control.text = event.text; invoke(control,'onChanging'); }
            else if (event.kind === 'click') {
              if (control === cancelElement) nativeFail('Script canceled');
              if (control.onClick) invoke(control,'onClick');
              else if (control === cancelElement) raw.close(0);
              else if (control === defaultElement) raw.close(1);
            }
          }
        }
      } finally { visibleWindow = null; }
      return returnCode;
    };
    return win;
  }
  function expose(name,value) { Object.defineProperty(global,name,{value,writable:false,configurable:false,enumerable:true}); }
  for (const [name,value] of Object.entries({app:strict(app,'app'),CompItem,FootageItem,FolderItem,Layer,AVLayer,TextLayer,TextDocument,MarkerValue,KeyframeEase,Window,ScriptUI:strict({newFont:font},'ScriptUI'),PropertyValueType:strict(Object.freeze({TwoD_SPATIAL:'TwoD_SPATIAL',ThreeD_SPATIAL:'ThreeD_SPATIAL',TwoD:'TwoD',ThreeD:'ThreeD',OneD:'OneD',TEXT_DOCUMENT:'TEXT_DOCUMENT',MARKER:'MARKER'}),'PropertyValueType'),KeyframeInterpolationType:strict(Object.freeze({LINEAR:'LINEAR',BEZIER:'BEZIER',HOLD:'HOLD'}),'KeyframeInterpolationType'),alert(message){ui({kind:'alert',message:boundedText(message,65536)});},confirm(message){return ui({kind:'confirm',message:boundedText(message,65536)}).value;},console:strict({log(){result(nativePrint(Array.from(arguments).map(String).join(' ')));}},'console'),$:strict({writeln(message){result(nativePrint(String(message)));}},'$')})) expose(name,value);
  for (const name of ['File','Folder','Socket','system','BridgeTalk','ExternalObject','XML','XMLList','fetch','XMLHttpRequest','WebSocket','setTimeout','setInterval','require','process']) Object.defineProperty(global,name,{get(){return unsupported(name);},set(){return unsupported(name);},configurable:false});
  delete global.__le_host; delete global.__le_ui; delete global.__le_fail; delete global.__le_print;
  return function finalize() { if (undoDepth !== 0) fail('Script ended with an unclosed undo group'); };
})(globalThis,__le_host,__le_ui,__le_fail,__le_print);
