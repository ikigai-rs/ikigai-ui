// ikigai-ui's WebGPU scene renderer (ledger #3514), a first-party script like shell.js.
//
// For every `figure[data-ui-scene]` the html face writes for a `scene` node, it reads the one
// scene datum the figure names (a same-origin path, nothing else) and draws it with fixed
// shaders: nodes as discs, edges as lines. Sealed: it fetches nothing the datum does not name,
// and the datum names nothing to fetch. With no WebGPU, no adapter, a datum that cannot be read
// or no script at all, the canvas stays hidden and the figure's replacement (the same graph as
// text, always on the page) is the scene.
//
// Interaction, mouse AND keyboard: drag or arrow keys orbit, the wheel or + and - zoom, a click
// or N (Shift+N back) selects a node, Escape clears it. The selection is said in #status.
//
// ONE scene, two faces: NODE_WGSL and EDGE_WGSL below are also the GPUI face's shaders (ledger
// #3516), which `ikigai_ui::scene::wgsl` reads out of this file. Keep each a `var X_WGSL =` run of
// double-quoted string literals joined by `+`, with no escapes, ending in `;` at a line's end.
// Colors are the palette's, by role, in the uniform: nodes `--accent`, edges `--line`, the
// selection `--focus` (the ground is the page's own `--bg`, through the transparent canvas).
(function () {
  "use strict";
  var NODE_WGSL =
    "struct U { m: mat4x4<f32>, aspect: f32, size: f32, sel: f32, pad: f32, node: vec4<f32>, edge: vec4<f32>, chosen: vec4<f32> };" +
    "@group(0) @binding(0) var<uniform> u: U;" +
    "struct O { @builtin(position) p: vec4<f32>, @location(0) q: vec2<f32>, @location(1) s: f32 };" +
    "@vertex fn vs(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32, @location(0) at: vec3<f32>) -> O {" +
    "  var c = array<vec2<f32>, 6>(vec2(-1.,-1.), vec2(1.,-1.), vec2(-1.,1.), vec2(-1.,1.), vec2(1.,-1.), vec2(1.,1.));" +
    "  var o: O; let p = u.m * vec4(at, 1.0);" +
    "  let k = select(1.0, 1.8, f32(i) == u.sel);" +
    "  o.p = p + vec4(c[v].x * u.size * k / u.aspect, c[v].y * u.size * k, 0.0, 0.0) * p.w;" +
    "  o.q = c[v]; o.s = select(0.0, 1.0, f32(i) == u.sel); return o; }" +
    "@fragment fn fs(o: O) -> @location(0) vec4<f32> {" +
    "  if (dot(o.q, o.q) > 1.0) { discard; }" +
    "  return mix(u.node, u.chosen, o.s); }";
  var EDGE_WGSL =
    "struct U { m: mat4x4<f32>, aspect: f32, size: f32, sel: f32, pad: f32, node: vec4<f32>, edge: vec4<f32>, chosen: vec4<f32> };" +
    "@group(0) @binding(0) var<uniform> u: U;" +
    "@vertex fn vs(@location(0) at: vec3<f32>) -> @builtin(position) vec4<f32> { return u.m * vec4(at, 1.0); }" +
    "@fragment fn fs() -> @location(0) vec4<f32> { return u.edge; }";

  // A palette token as sRGB channels in [0, 1] (the canvas format is unorm, so they land as
  // written), else the fallback.
  function token(el, name, fallback) {
    var v = getComputedStyle(el).getPropertyValue("--" + name).trim();
    var m = /^#([0-9a-fA-F]{6})$/.exec(v);
    if (!m) return fallback;
    var n = parseInt(m[1], 16);
    return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, 1];
  }

  function say(text) {
    var status = document.getElementById("status");
    if (status) status.textContent = text;
  }

  function perspective(fov, aspect, near, far) {
    var f = 1 / Math.tan((fov * Math.PI) / 360), r = 1 / (near - far);
    return [f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, far * r, -1, 0, 0, near * far * r, 0];
  }
  function multiply(a, b) {
    var o = new Array(16);
    for (var c = 0; c < 4; c++)
      for (var r = 0; r < 4; r++) {
        var s = 0;
        for (var k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
        o[c * 4 + r] = s;
      }
    return o;
  }
  function view(yaw, pitch, distance) {
    var cy = Math.cos(yaw), sy = Math.sin(yaw), cp = Math.cos(pitch), sp = Math.sin(pitch);
    // Rotate about y, then x, then step back along z.
    return [cy, sp * sy, -cp * sy, 0, 0, cp, sp, 0, sy, -sp * cy, cp * cy, 0, 0, 0, -distance, 1];
  }
  function project(m, p) {
    var x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12];
    var y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13];
    var w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
    return [x / w, y / w];
  }

  // Only a same-origin path, and only a scene datum: never a URL from anywhere else. A leading
  // `/` and no second one is not enough: the URL parser reads `/\host` and `/<tab>/host` as
  // `//host` (ledger #3525), so the path is resolved and its origin compared.
  function datumUrl(figure) {
    var path = figure.getAttribute("data-ui-scene") || "";
    if (path.charAt(0) !== "/" || path.charAt(1) === "/") return null;
    var url;
    try {
      url = new URL(path, window.location.href);
    } catch (e) {
      return null;
    }
    return url.origin === window.location.origin ? url.href : null;
  }

  async function draw(figure) {
    var canvas = figure.querySelector("canvas.ui-scene-canvas");
    var url = datumUrl(figure);
    if (!canvas || !url || !navigator.gpu) return;
    var adapter = await navigator.gpu.requestAdapter();
    if (!adapter) return;
    var device = await adapter.requestDevice();
    var response = await fetch(url, { headers: { Accept: "application/vnd.ikigai.ui.scene+json" }, credentials: "same-origin" });
    if (!response.ok) return;
    var scene = await response.json();
    if (!scene || !Array.isArray(scene.nodes) || !Array.isArray(scene.edges)) return;
    var context = canvas.getContext("webgpu");
    if (!context) return;
    var format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({ device: device, format: format, alphaMode: "premultiplied" });

    var index = {};
    var points = new Float32Array(scene.nodes.length * 3);
    scene.nodes.forEach(function (n, i) {
      index[n.id] = i;
      points.set(n.position.slice(0, 3), i * 3);
    });
    var lines = [];
    scene.edges.forEach(function (e) {
      if (e.from in index && e.to in index)
        lines.push.apply(lines, scene.nodes[index[e.from]].position.concat(scene.nodes[index[e.to]].position));
    });
    lines = new Float32Array(lines.length ? lines : [0, 0, 0, 0, 0, 0]);
    function buffer(data, usage) {
      var b = device.createBuffer({ size: Math.max(16, data.byteLength), usage: usage | GPUBufferUsage.COPY_DST });
      device.queue.writeBuffer(b, 0, data);
      return b;
    }
    var nodeBuffer = buffer(points.length ? points : new Float32Array(3), GPUBufferUsage.VERTEX);
    var edgeBuffer = buffer(lines, GPUBufferUsage.VERTEX);
    var uniform = device.createBuffer({ size: 128, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    var layoutGroup = device.createBindGroupLayout({ entries: [{ binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT, buffer: {} }] });
    var group = device.createBindGroup({ layout: layoutGroup, entries: [{ binding: 0, resource: { buffer: uniform } }] });
    var pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [layoutGroup] });
    function pipeline(code, stepMode, topology) {
      var module = device.createShaderModule({ code: code });
      return device.createRenderPipeline({
        layout: pipelineLayout,
        vertex: { module: module, entryPoint: "vs", buffers: [{ arrayStride: 12, stepMode: stepMode, attributes: [{ shaderLocation: 0, offset: 0, format: "float32x3" }] }] },
        fragment: { module: module, entryPoint: "fs", targets: [{ format: format }] },
        primitive: { topology: topology },
      });
    }
    var nodes = pipeline(NODE_WGSL, "instance", "triangle-list");
    var edges = pipeline(EDGE_WGSL, "vertex", "line-list");

    var camera = scene.camera || { position: [0, 0, 3], fov: 50 };
    var state = { yaw: 0.6, pitch: 0.3, distance: Math.abs(camera.position[2]) || 3, selected: -1, matrix: null };
    function render() {
      var aspect = canvas.width / canvas.height;
      state.matrix = multiply(perspective(camera.fov || 50, aspect, 0.1, 100), view(state.yaw, state.pitch, state.distance));
      var u = new Float32Array(32);
      u.set(state.matrix, 0);
      u[16] = aspect; u[17] = 0.03; u[18] = state.selected;
      // Read each frame, so a theme change repaints with the new palette.
      u.set(token(figure, "accent", [0.15, 0.45, 0.85, 1]), 20);
      u.set(token(figure, "line", [0.5, 0.5, 0.55, 1]), 24);
      u.set(token(figure, "focus", [0.9, 0.4, 0.1, 1]), 28);
      device.queue.writeBuffer(uniform, 0, u);
      var encoder = device.createCommandEncoder();
      var pass = encoder.beginRenderPass({
        colorAttachments: [{ view: context.getCurrentTexture().createView(), clearValue: { r: 0, g: 0, b: 0, a: 0 }, loadOp: "clear", storeOp: "store" }],
      });
      pass.setBindGroup(0, group);
      pass.setPipeline(edges);
      pass.setVertexBuffer(0, edgeBuffer);
      pass.draw(lines.length / 3);
      pass.setPipeline(nodes);
      pass.setVertexBuffer(0, nodeBuffer);
      pass.draw(6, scene.nodes.length);
      pass.end();
      device.queue.submit([encoder.finish()]);
    }
    function select(i) {
      state.selected = i;
      render();
      if (i < 0) return say("No node selected.");
      var n = scene.nodes[i];
      var out = scene.edges.filter(function (e) { return e.from === n.id; }).length;
      var into = scene.edges.filter(function (e) { return e.to === n.id; }).length;
      say("Selected " + n.label + (n.type ? " (" + n.type.replace(/^.*[#/:]/, "") + ")" : "") + ": " + out + " out, " + into + " in.");
    }

    var drag = null;
    canvas.addEventListener("pointerdown", function (e) { drag = { x: e.clientX, y: e.clientY, moved: false }; canvas.setPointerCapture(e.pointerId); });
    canvas.addEventListener("pointermove", function (e) {
      if (!drag) return;
      var dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 2) drag.moved = true;
      state.yaw += dx * 0.01;
      state.pitch = Math.max(-1.5, Math.min(1.5, state.pitch + dy * 0.01));
      drag.x = e.clientX; drag.y = e.clientY;
      render();
    });
    canvas.addEventListener("pointerup", function (e) {
      var was = drag; drag = null;
      if (!was || was.moved) return;
      var r = canvas.getBoundingClientRect();
      var x = ((e.clientX - r.left) / r.width) * 2 - 1, y = 1 - ((e.clientY - r.top) / r.height) * 2;
      var best = -1, bestD = 0.0025;
      scene.nodes.forEach(function (n, i) {
        var p = project(state.matrix, n.position), d = (p[0] - x) * (p[0] - x) + (p[1] - y) * (p[1] - y);
        if (d < bestD) { best = i; bestD = d; }
      });
      select(best);
    });
    canvas.addEventListener("wheel", function (e) {
      e.preventDefault();
      state.distance = Math.max(1, Math.min(20, state.distance * (e.deltaY > 0 ? 1.1 : 0.9)));
      render();
    }, { passive: false });
    canvas.addEventListener("keydown", function (e) {
      var n = scene.nodes.length;
      switch (e.key) {
        case "ArrowLeft": state.yaw -= 0.1; break;
        case "ArrowRight": state.yaw += 0.1; break;
        case "ArrowUp": state.pitch = Math.max(-1.5, state.pitch - 0.1); break;
        case "ArrowDown": state.pitch = Math.min(1.5, state.pitch + 0.1); break;
        case "+": case "=": state.distance = Math.max(1, state.distance * 0.9); break;
        case "-": state.distance = Math.min(20, state.distance * 1.1); break;
        case "n": case "N":
          if (n) select(e.shiftKey ? (state.selected - 1 + n) % n : (state.selected + 1) % n);
          e.preventDefault();
          return;
        case "Escape": select(-1); e.preventDefault(); return;
        default: return;
      }
      e.preventDefault();
      render();
    });

    canvas.hidden = false;
    figure.setAttribute("data-ui-scene-drawn", "");
    render();
  }

  function start() {
    var figures = document.querySelectorAll("figure[data-ui-scene]");
    for (var i = 0; i < figures.length; i++) {
      draw(figures[i]).catch(function () { /* the replacement stays the scene */ });
    }
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
