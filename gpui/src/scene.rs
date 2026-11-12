//! The `scene` kind drawn offscreen (ledger #3516, phase 2 of #3514), behind the `scene` feature.
//!
//! GPUI 0.2 has no surface for a foreign GPU context, so the scene datum is rendered with `wgpu`
//! into an offscreen texture, read back and painted as an image. The shaders are the html face's
//! own ([`ikigai_ui::scene::wgsl`], read out of `urn:iki:ui:scene-renderer.js`), and so is the
//! camera: the same yaw, pitch and distance, the same keys, the same matrices. Colors are the
//! shared palette's by role: the ground `--bg`, nodes `--accent`, edges `--line`, the selection
//! `--focus` (and drawn 1.8 times larger, so the selection is never color alone).
use ikigai_ui::scene::{Scene, Wgsl};

/// The palette's colors by role, `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneColors {
    /// The ground (`--bg`).
    pub ground: u32,
    /// A node (`--accent`).
    pub node: u32,
    /// An edge (`--line`).
    pub edge: u32,
    /// The selected node (`--focus`).
    pub selected: u32,
}

impl SceneColors {
    /// The roles from a palette's tokens.
    pub fn from_tokens(tokens: &crate::theme::Tokens) -> Self {
        Self {
            ground: tokens.get("bg"),
            node: tokens.get("accent"),
            edge: tokens.get("line"),
            selected: tokens.get("focus"),
        }
    }
}

/// Where the camera is and what is selected: the html renderer's state, key for key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// About y, radians.
    pub yaw: f32,
    /// About x, radians, in [-1.5, 1.5].
    pub pitch: f32,
    /// From the target, in [1, 20].
    pub distance: f32,
    /// The selected node's index, if any.
    pub selected: Option<usize>,
}

impl View {
    /// The html renderer's starting view of `scene`.
    pub fn new(scene: &Scene) -> Self {
        let d = scene.camera.position[2].abs() as f32;
        Self {
            yaw: 0.6,
            pitch: 0.3,
            distance: if d > 0.0 { d } else { 3.0 },
            selected: None,
        }
    }

    /// A key as the html renderer reads it (arrows orbit, `+`/`=`/`-` zoom, `n` and Shift+`n`
    /// select the next and previous node, `escape` clears). Whether the key was the scene's.
    pub fn key(&mut self, key: &str, shift: bool, nodes: usize) -> bool {
        match key {
            "left" => self.yaw -= 0.1,
            "right" => self.yaw += 0.1,
            "up" => self.pitch = (self.pitch - 0.1).max(-1.5),
            "down" => self.pitch = (self.pitch + 0.1).min(1.5),
            "+" | "=" => self.distance = (self.distance * 0.9).max(1.0),
            "-" => self.distance = (self.distance * 1.1).min(20.0),
            "n" if nodes > 0 => {
                self.selected = Some(match (self.selected, shift) {
                    (None, false) => 0,
                    (None, true) => nodes - 1,
                    (Some(i), false) => (i + 1) % nodes,
                    (Some(i), true) => (i + nodes - 1) % nodes,
                })
            }
            "escape" => self.selected = None,
            _ => return false,
        }
        true
    }

    /// The view-projection matrix (column-major) at `aspect`, as the html renderer builds it.
    pub fn matrix(&self, scene: &Scene, aspect: f32) -> [f32; 16] {
        let fov = if scene.camera.fov > 0.0 {
            scene.camera.fov as f32
        } else {
            50.0
        };
        multiply(
            &perspective(fov, aspect, 0.1, 100.0),
            &view(self.yaw, self.pitch, self.distance),
        )
    }
}

/// What the status line says of the selection: the html renderer's sentence.
pub fn announce(scene: &Scene, selected: Option<usize>) -> String {
    let Some(n) = selected.and_then(|i| scene.nodes.get(i)) else {
        return "No node selected.".into();
    };
    let out = scene.edges.iter().filter(|e| e.from == n.id).count();
    let into = scene.edges.iter().filter(|e| e.to == n.id).count();
    let kind = n
        .r#type
        .as_deref()
        .map(|t| format!(" ({})", t.rsplit(['#', '/', ':']).next().unwrap_or(t)))
        .unwrap_or_default();
    format!("Selected {}{kind}: {out} out, {into} in.", n.label)
}

fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov * std::f32::consts::PI / 360.0).tan();
    let r = 1.0 / (near - far);
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far * r,
        -1.0,
        0.0,
        0.0,
        near * far * r,
        0.0,
    ]
}

fn multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

fn view(yaw: f32, pitch: f32, distance: f32) -> [f32; 16] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    [
        cy,
        sp * sy,
        -cp * sy,
        0.0,
        0.0,
        cp,
        sp,
        0.0,
        sy,
        -sp * cy,
        cp * cy,
        0.0,
        0.0,
        0.0,
        -distance,
        1.0,
    ]
}

/// Where node `i` lands in a `width` x `height` frame, in pixels from the top left.
pub fn pixel(scene: &Scene, view: &View, i: usize, width: u32, height: u32) -> Option<(u32, u32)> {
    let m = view.matrix(scene, width as f32 / height as f32);
    let p = scene.nodes.get(i)?.position.map(|x| x as f32);
    let x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12];
    let y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13];
    let w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
    let (x, y) = (x / w, y / w);
    let px = (x + 1.0) / 2.0 * width as f32;
    let py = (1.0 - y) / 2.0 * height as f32;
    (px >= 0.0 && py >= 0.0 && px < width as f32 && py < height as f32)
        .then_some((px as u32, py as u32))
}

fn floats(data: &[f32]) -> Vec<u8> {
    data.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn channels(hex: u32) -> [f32; 4] {
    let c = |s: u32| ((hex >> s) & 0xff) as f32 / 255.0;
    [c(16), c(8), c(0), 1.0]
}

/// A second GPU device beside GPUI's, with the html face's two pipelines.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    nodes: wgpu::RenderPipeline,
    edges: wgpu::RenderPipeline,
}

/// A scene's vertex buffers, made once per datum.
pub struct Prepared {
    nodes: wgpu::Buffer,
    node_count: u32,
    edges: wgpu::Buffer,
    edge_vertices: u32,
}

/// Frames are BGRA, 8 bits a channel, unorm: the palette's sRGB values land as written, and
/// GPUI's images are BGRA.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

impl Renderer {
    /// A device and the pipelines, or why there is none (no adapter: the replacement stays).
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL | wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|e| format!("no GPU device: {e}"))?;
        let Wgsl { node, edge } = ikigai_ui::scene::wgsl();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = |code: &str, step: wgpu::VertexStepMode, topology| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(code.into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 12,
                        step_mode: step,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        }],
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let nodes = pipeline(
            &node,
            wgpu::VertexStepMode::Instance,
            wgpu::PrimitiveTopology::TriangleList,
        );
        let edges = pipeline(
            &edge,
            wgpu::VertexStepMode::Vertex,
            wgpu::PrimitiveTopology::LineList,
        );
        Ok(Self {
            device,
            queue,
            layout,
            nodes,
            edges,
        })
    }

    fn buffer(&self, bytes: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        let b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes.len().max(16) as u64,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&b, 0, bytes);
        b
    }

    /// `scene`'s vertex buffers: a point per node, a line per edge between two of its nodes.
    pub fn prepare(&self, scene: &Scene) -> Prepared {
        let index: std::collections::HashMap<&str, usize> = scene
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let at = |i: usize| scene.nodes[i].position.map(|x| x as f32);
        let points: Vec<f32> = (0..scene.nodes.len()).flat_map(at).collect();
        let lines: Vec<f32> = scene
            .edges
            .iter()
            .filter_map(|e| Some((*index.get(e.from.as_str())?, *index.get(e.to.as_str())?)))
            .flat_map(|(a, b)| at(a).into_iter().chain(at(b)))
            .collect();
        Prepared {
            nodes: self.buffer(&floats(&points), wgpu::BufferUsages::VERTEX),
            node_count: scene.nodes.len() as u32,
            edges: self.buffer(&floats(&lines), wgpu::BufferUsages::VERTEX),
            edge_vertices: (lines.len() / 3) as u32,
        }
    }

    /// One frame of `scene` at `view`, `width` x `height`, read back: BGRA rows, top first,
    /// `width * 4` bytes each.
    pub fn frame(
        &self,
        scene: &Scene,
        prepared: &Prepared,
        view: &View,
        colors: &SceneColors,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let aspect = width as f32 / height as f32;
        let mut u = [0f32; 32];
        u[..16].copy_from_slice(&view.matrix(scene, aspect));
        u[16] = aspect;
        u[17] = 0.03;
        u[18] = view.selected.map_or(-1.0, |i| i as f32);
        u[20..24].copy_from_slice(&channels(colors.node));
        u[24..28].copy_from_slice(&channels(colors.edge));
        u[28..32].copy_from_slice(&channels(colors.selected));
        let uniform = self.buffer(&floats(&u), wgpu::BufferUsages::UNIFORM);
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene frame"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = texture.create_view(&Default::default());
        let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene readback"),
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let ground = channels(colors.ground).map(f64::from);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: ground[0],
                            g: ground[1],
                            b: ground[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &group, &[]);
            pass.set_pipeline(&self.edges);
            pass.set_vertex_buffer(0, prepared.edges.slice(..));
            pass.draw(0..prepared.edge_vertices, 0..1);
            pass.set_pipeline(&self.nodes);
            pass.set_vertex_buffer(0, prepared.nodes.slice(..));
            pass.draw(0..6, 0..prepared.node_count);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait)
            .expect("the GPU finishes the frame");
        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            let start = y * row as usize;
            out.extend_from_slice(&mapped[start..start + width as usize * 4]);
        }
        drop(mapped);
        readback.unmap();
        out
    }
}
