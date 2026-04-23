//! Canvas API stubs — getContext('2d'), Path2D, CanvasRenderingContext2D.
//!
//! Returns stub objects with no-op draw methods for feature detection.
//! No actual rendering — this is SSR.

use crate::dom::node::NodeData;
use crate::js::templates::{arena_ref, unwrap_node_id};

/// Install getContext on the Element ObjectTemplate (called during template setup).
pub fn install_on_element(scope: &mut v8::PinnedRef<v8::HandleScope<()>>, proto: &v8::Local<v8::ObjectTemplate>) {
    use super::helpers::set_method;
    set_method(scope, proto, "getContext", get_context);
    log::debug!("Installed getContext on Element prototype");
}

/// Install Path2D and other canvas globals.
pub fn install_globals(scope: &mut v8::PinnedRef<v8::HandleScope>, global: v8::Local<v8::Object>) {
    let path2d = v8::Function::new(scope, |_scope: &mut v8::PinnedRef<v8::HandleScope>,
        _args: v8::FunctionCallbackArguments, _rv: v8::ReturnValue| {
        // Path2D constructor stub — returns empty object (this)
    }).unwrap();
    let k = v8::String::new(scope, "Path2D").unwrap();
    global.set(scope, k.into(), path2d.into());
    log::debug!("Installed Path2D constructor");
}

/// HTMLCanvasElement.getContext(contextType) — returns 2D stub or null.
fn get_context(
    scope: &mut v8::PinnedRef<v8::HandleScope>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(node_id) = unwrap_node_id(scope, args.this()) else { return };
    let arena = arena_ref(scope);

    // Only <canvas> elements have getContext
    let is_canvas = matches!(&arena.nodes[node_id].data,
        NodeData::Element(d) if &*d.name.local == "canvas"
    );
    if !is_canvas {
        rv.set(v8::null(scope).into());
        return;
    }

    // Ensure canvas has default width/height IDL properties (spec: 300x150)
    let this = args.this();
    let w_key = v8::String::new(scope, "width").unwrap();
    if this.get(scope, w_key.into()).map(|v| v.is_undefined()).unwrap_or(true) {
        let v = v8::Integer::new(scope, 300);
        this.set(scope, w_key.into(), v.into());
    }
    let h_key = v8::String::new(scope, "height").unwrap();
    if this.get(scope, h_key.into()).map(|v| v.is_undefined()).unwrap_or(true) {
        let v = v8::Integer::new(scope, 150);
        this.set(scope, h_key.into(), v.into());
    }

    let context_type = args.get(0).to_rust_string_lossy(scope);
    match context_type.as_str() {
        "2d" => {
            log::trace!("canvas.getContext('2d') — returning stub context");
            let ctx = create_2d_context(scope);
            rv.set(ctx.into());
        }
        _ => {
            // webgl, webgl2, bitmaprenderer, webgpu — not supported in SSR
            log::trace!("canvas.getContext('{}') — returning null (not supported)", context_type);
            rv.set(v8::null(scope).into());
        }
    }
}

/// Create a stub CanvasRenderingContext2D with no-op draw methods.
fn create_2d_context<'s, 'i>(scope: &mut v8::PinnedRef<'s, v8::HandleScope<'i>>) -> v8::Local<'s, v8::Object> {
    let ctx = v8::Object::new(scope);

    // No-op draw methods
    let noop = v8::Function::new(scope, |_: &mut v8::PinnedRef<v8::HandleScope>,
        _: v8::FunctionCallbackArguments, _: v8::ReturnValue| {}).unwrap();

    for name in &[
        "fillRect", "strokeRect", "clearRect",
        "beginPath", "closePath", "fill", "stroke", "clip",
        "moveTo", "lineTo", "arc", "arcTo", "ellipse", "rect", "roundRect",
        "bezierCurveTo", "quadraticCurveTo",
        "fillText", "strokeText",
        "drawImage",
        "save", "restore", "reset",
        "translate", "rotate", "scale", "transform", "setTransform", "resetTransform",
        "createLinearGradient", "createRadialGradient", "createConicGradient", "createPattern",
        "putImageData",
        "setLineDash",
        "drawFocusIfNeeded",
    ] {
        let k = v8::String::new(scope, name).unwrap();
        ctx.set(scope, k.into(), noop.into());
    }

    // measureText(text) — returns {width: 0}
    let measure = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let result = v8::Object::new(scope);
        let k = v8::String::new(scope, "width").unwrap();
        let v = v8::Number::new(scope, 0.0);
        result.set(scope, k.into(), v.into());
        rv.set(result.into());
    }).unwrap();
    let k = v8::String::new(scope, "measureText").unwrap();
    ctx.set(scope, k.into(), measure.into());

    // getImageData — returns stub {data: Uint8ClampedArray, width, height}
    let get_image = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let w = args.get(2).uint32_value(scope).unwrap_or(1) as usize;
        let h = args.get(3).uint32_value(scope).unwrap_or(1) as usize;
        let result = v8::Object::new(scope);
        let buf = v8::ArrayBuffer::new(scope, w * h * 4);
        let arr = v8::Uint8Array::new(scope, buf, 0, w * h * 4).unwrap();
        let k = v8::String::new(scope, "data").unwrap();
        result.set(scope, k.into(), arr.into());
        let k = v8::String::new(scope, "width").unwrap();
        let v = v8::Number::new(scope, w as f64);
        result.set(scope, k.into(), v.into());
        let k = v8::String::new(scope, "height").unwrap();
        let v = v8::Number::new(scope, h as f64);
        result.set(scope, k.into(), v.into());
        rv.set(result.into());
    }).unwrap();
    let k = v8::String::new(scope, "getImageData").unwrap();
    ctx.set(scope, k.into(), get_image.into());

    // createImageData — same as getImageData
    let k = v8::String::new(scope, "createImageData").unwrap();
    ctx.set(scope, k.into(), get_image.into());

    // getTransform — returns DOMMatrix-like stub
    let get_transform = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        let m = v8::Object::new(scope);
        for (name, val) in &[("a", 1.0), ("b", 0.0), ("c", 0.0), ("d", 1.0), ("e", 0.0), ("f", 0.0)] {
            let k = v8::String::new(scope, name).unwrap();
            let v = v8::Number::new(scope, *val);
            m.set(scope, k.into(), v.into());
        }
        rv.set(m.into());
    }).unwrap();
    let k = v8::String::new(scope, "getTransform").unwrap();
    ctx.set(scope, k.into(), get_transform.into());

    // getLineDash — returns empty array
    let get_dash = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Array::new(scope, 0).into());
    }).unwrap();
    let k = v8::String::new(scope, "getLineDash").unwrap();
    ctx.set(scope, k.into(), get_dash.into());

    // isPointInPath / isPointInStroke — return false
    let false_fn = v8::Function::new(scope, |scope: &mut v8::PinnedRef<v8::HandleScope>,
        _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue| {
        rv.set(v8::Boolean::new(scope, false).into());
    }).unwrap();
    let k = v8::String::new(scope, "isPointInPath").unwrap();
    ctx.set(scope, k.into(), false_fn.into());
    let k = v8::String::new(scope, "isPointInStroke").unwrap();
    ctx.set(scope, k.into(), false_fn.into());
    let k = v8::String::new(scope, "isContextLost").unwrap();
    ctx.set(scope, k.into(), false_fn.into());

    // Style properties (defaults)
    let k = v8::String::new(scope, "fillStyle").unwrap();
    let v = v8::String::new(scope, "#000000").unwrap();
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "strokeStyle").unwrap();
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "lineWidth").unwrap();
    let v = v8::Number::new(scope, 1.0);
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "globalAlpha").unwrap();
    let v = v8::Number::new(scope, 1.0);
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "font").unwrap();
    let v = v8::String::new(scope, "10px sans-serif").unwrap();
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "textAlign").unwrap();
    let v = v8::String::new(scope, "start").unwrap();
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "textBaseline").unwrap();
    let v = v8::String::new(scope, "alphabetic").unwrap();
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "lineDashOffset").unwrap();
    let v = v8::Number::new(scope, 0.0);
    ctx.set(scope, k.into(), v.into());
    let k = v8::String::new(scope, "imageSmoothingEnabled").unwrap();
    let v = v8::Boolean::new(scope, true);
    ctx.set(scope, k.into(), v.into());

    // canvas back-reference (null for stub)
    let k = v8::String::new(scope, "canvas").unwrap();
    let null = v8::null(scope);
    ctx.set(scope, k.into(), null.into());

    ctx
}
