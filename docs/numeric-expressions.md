# Author numeric expressions in the editor

Use a **2D composition** for Position, Scale, Opacity and Slider Control
expressions. These controls work from a new native project; a setup script or
generated project is not required. Text/path expressions and expressions in a
composition containing spatial layers are separate, unsupported targets.

## Create a control and connect a layer

1. Choose **Layer → New null object**. Rename it `Controls` in the Timeline and
   press Enter to finish the name edit.
2. Select that Null and add **Slider Control** from the Effect menu or
   Effects & Presets. Effect Controls now exposes its name, value and key button.
   Rename the control `Offset`, confirming the name with Enter.
3. Create/select a rectangle or text layer. Click **fx+** beside Position in
   Properties. The editor starts with `value`, the authored pre-expression sample.
4. Replace the draft with this independently authored example:

```js
var offset = thisComp.layer("Controls").effect("Offset")(1);
[value[0] + offset, value[1]];
```

5. Leave Enabled checked and choose **Apply**. After the current-frame check
   succeeds, the source is saved as one Undo step. Change Offset in Effect Controls
   to move the layer while its authored Position values remain intact. The Slider
   can be keyed with its ordinary key control.

Position and Scale return two-number arrays; Scale uses percentages. Opacity and
Slider controls return one finite number. Opacity uses percentages and clips only
at paint. The **fx** button reopens an existing program. Slider Amount also has
an expression button, including on Null objects.

## Edit, disable and remove

- The source and Enabled checkbox are drafts until Apply. Cancel/Escape keeps
  source and history unchanged. Ordinary text Undo/Redo stays inside the field.
- Ctrl+Enter, or Cmd+Enter where supported, requests Apply; ordinary Enter inserts
  a newline. Finish an existing property/name field before opening an expression.
- Unchecking Enabled and applying stores a nonempty draft without running it.
  This is useful while building controls referenced by a program. An explicit
  **Remove expression** button deletes the program and returns to authored values.
- Untouched Apply preserves existing source bytes, including CRLF line endings.
  No evaluated values replace the authored base tracks or keyframes.
- Use distinct Slider names. A control shadowed by an earlier same-name Slider
  must be renamed before its expression can be edited.

## Understand checks and errors

Enabled Apply checks the current preview frame and the edited property using
the existing bounded evaluator outside the UI process. Syntax, missing-reference,
cycle and invalid-result errors remain visible in the editor; the rejected draft
does not change the project. A visible dependency elsewhere in that frame can
also be the source of an error. Editing or canceling retires an in-flight check.

The check is for the current frame. Preview/export still report failures that
occur at other times. Expressions read supported project values; they do not get
filesystem, network or shell access. Source is limited to 16,384 UTF-8 bytes.

This feature adds practical numeric authoring. It does not establish replacement
of an entire After Effects project, source-text/mask expression support, general
3D authoring, or equivalent effects and typography. The exact platform/native
qualification is recorded in `apps/desktop/NUMERIC_EXPRESSION_AUTHORING.md`;
Windows and marked-IME behavior are not implied by a Linux source/build check.
