#target aftereffects
// Independently authored native XYZ example. Open the synthetic native depth
// fixture first; its explicit camera is part of the project, not guessed here.
(function () {
    var comp = app.project.activeItem;
    if (!(comp instanceof CompItem) || comp.numLayers < 1) {
        throw Error('Open a native composition with a joined XYZ layer first.');
    }
    var layer = comp.layer(1), position = layer.transform.position;
    if (!layer.threeDLayer || position.propertyValueType !== PropertyValueType.ThreeD_SPATIAL) {
        throw Error('The top layer must have native joined XYZ Position.');
    }
    if (position.numKeys !== 0) {
        throw Error('Use an unkeyed XYZ layer for this example.');
    }
    var dialog = new Window('dialog', 'Native XYZ example');
    dialog.orientation = 'column';
    dialog.add('statictext', undefined, 'Write two XYZ keys and retain dormant endpoint metadata.');
    var buttons = dialog.add('group');
    var apply = buttons.add('button', undefined, 'Apply');
    var cancel = buttons.add('button', undefined, 'Cancel');
    apply.onClick = function () {
        app.beginUndoGroup('Native XYZ example');
        position.setValueAtTime(0, [340, 135, 500]);
        position.setValueAtTime(1, [300, 145, 0]);
        for (var i = 1; i <= 2; i++) {
            position.setInterpolationTypeAtKey(i, KeyframeInterpolationType.BEZIER, KeyframeInterpolationType.BEZIER);
            position.setTemporalEaseAtKey(i,
                [new KeyframeEase(i === 1 ? 1e-199 : 0, 25)],
                [new KeyframeEase(i === 2 ? 2e-199 : 0, 75)]);
            position.setTemporalContinuousAtKey(i, false);
            position.setTemporalAutoBezierAtKey(i, false);
            position.setSpatialTangentsAtKey(i,
                i === 1 ? [-3, 4, -5] : [0, 0, 0],
                i === 2 ? [6, -8, 10] : [0, 0, 0]);
            position.setSpatialContinuousAtKey(i, true);
            position.setSpatialAutoBezierAtKey(i, false);
        }
        app.endUndoGroup();
        dialog.close(1);
    };
    cancel.onClick = function () { dialog.close(0); };
    dialog.defaultElement = apply;
    dialog.cancelElement = cancel;
    dialog.center();
    dialog.show();
}());
