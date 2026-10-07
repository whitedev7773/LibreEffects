#target aftereffects
// Libre Effects 2D sample. Select a static text layer before running this file.
// Creates one duplicate per nonempty line, with marker and opacity animation.
(function () {
    var comp = app.project.activeItem;
    if (!(comp instanceof CompItem) || comp.selectedLayers.length !== 1) {
        alert("Select one static text layer in the active composition first.");
        return;
    }
    var template = comp.selectedLayers[0];
    if (!(template instanceof TextLayer)) {
        alert("The selected layer must be a static text layer.");
        return;
    }
    var dialog = new Window("dialog", "Duplicate text · 2D example");
    dialog.orientation = "column";
    dialog.add("statictext", undefined, "One text layer per line. The template stays unchanged.");
    var text = dialog.add("edittext", undefined, "Hello\n안녕하세요", {multiline: true});
    text.active = true;
    text.preferredSize = [560, 140];
    var count = dialog.add("statictext", undefined, "2 lines");
    text.onChanging = function () { count.text = text.text.split(/\r?\n/).filter(function (line) { return line.trim().length > 0; }).length + " lines"; };
    var buttons = dialog.add("group");
    buttons.orientation = "row";
    var run = buttons.add("button", undefined, "Create layers", {name: "ok"});
    buttons.add("button", undefined, "Cancel", {name: "cancel"});
    run.onClick = function () { dialog.close(1); };
    if (dialog.show() !== 1) return;
    var lines = text.text.split(/\r?\n/).filter(function (line) { return line.trim().length > 0; });
    if (lines.length > 50) throw new Error("This example allows up to 50 lines.");
    var end = Math.round(Math.min(1, comp.duration - comp.frameDuration) / comp.frameDuration) * comp.frameDuration;
    app.beginUndoGroup("Duplicate text lines");
    for (var i = 0; i < lines.length; ++i) {
        var layer = template.duplicate();
        layer.name = "Script line " + (i + 1);
        var source = layer.property("Source Text");
        var document = source.value;
        document.text = lines[i];
        source.setValue(document);
        layer.moveToBeginning();
        layer.property("Marker").setValueAtTime(0, new MarkerValue("Focus"));
        var opacity = layer.transform.opacity;
        opacity.setValueAtTime(0, 0);
        if (end > 0) opacity.setValueAtTime(end, 100);
    }
    app.endUndoGroup();
    $.writeln("Created " + lines.length + " text layers");
})();
