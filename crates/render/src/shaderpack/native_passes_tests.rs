//! Exercises native transparency over a whole pack frame without game assets.

use super::*;
use crate::render_test_support::scene_raster as raster;
use bevy::{
    core_pipeline::core_3d::Transparent3d,
    render::{
        Render, RenderApp, RenderSystems,
        render_phase::{
            Draw, DrawError, DrawFunctions, PhaseItemExtraIndex, TrackedRenderPass,
            ViewSortedRenderPhases,
        },
        render_resource::CachedRenderPipelineId,
        renderer::{RenderAdapter, RenderDevice, RenderQueue},
        sync_world::MainEntity,
        view::ExtractedView,
    },
};
use std::sync::Arc;

#[derive(Resource)]
struct Tint(wgpu::RenderPipeline);

struct DrawTint;

impl Draw<Transparent3d> for DrawTint {
    fn draw<'w>(
        &mut self,
        world: &'w World,
        pass: &mut TrackedRenderPass<'w>,
        _view: Entity,
        _item: &Transparent3d,
    ) -> Result<(), DrawError> {
        let raw = pass.wgpu_pass();
        raw.set_pipeline(&world.resource::<Tint>().0);
        raw.draw(0..3, 0..1);
        Ok(())
    }
}

/// Queues a native translucent layer through the same phase as weather and particles.
fn queue_tint(
    views: Query<(Entity, &MainEntity, &ExtractedView)>,
    functions: Res<DrawFunctions<Transparent3d>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
) {
    let draw_function = functions.read().id::<DrawTint>();
    for (entity, main, view) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        phase.add_retained(Transparent3d {
            sorting_info: bevy::core_pipeline::core_3d::TransparentSortingInfo3d::AlwaysOnTop,
            distance: 0.0,
            entity: (entity, *main),
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: false,
        });
    }
}

/// Supplies empty ready terrain instead of requiring chunk carriers for this frame.
fn ready_terrain(mut commands: Commands, device: Res<RenderDevice>) {
    commands.insert_resource(super::super::geometry::ShaderpackGeometry::ready_fixture(
        &device,
    ));
}

/// Creates a depth-tested native alpha layer whose colour differs from the pack sky.
fn tint_pipeline(device: &RenderDevice) -> wgpu::RenderPipeline {
    let shader = device
        .wgpu_device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native pack-composition fixture"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4(uv * 2.0 - 1.0, 0.5, 1.0);
}
@fragment fn fragment() -> @location(0) vec4<f32> { return vec4(1.0, 0.0, 0.0, 0.5); }
"#
                .into(),
            ),
        });
    device
        .wgpu_device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("native pack-composition fixture"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_scene::SCENE_COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: bevy::core_pipeline::core_3d::CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: default(),
                bias: default(),
            }),
            multisample: default(),
            multiview_mask: None,
            cache: None,
        })
}

#[test]
#[allow(clippy::print_stderr, reason = "names the missing native GPU fixture")]
fn native_transparency_preserves_pack_colour_through_final() {
    let Some(renderer) = raster::renderer() else {
        eprintln!(
            "skipping native_transparency_preserves_pack_colour_through_final: missing native GPU fixture"
        );
        return;
    };
    let (mut app, camera, output) = raster::app_with_effects(renderer, |app| {
        app.add_plugins(super::super::ShaderPackRenderPlugin);
        crate::chunk::transparent::gamma_pass::register_fixture_commands(app);
        app.sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<DrawFunctions<Transparent3d>>()
            .write()
            .add_with::<DrawTint, _>(DrawTint);
        app.sub_app_mut(RenderApp).add_systems(
            Render,
            (
                ready_terrain
                    .after(super::super::geometry::prepare_shaderpack_geometry)
                    .before(super::super::plugin::configure_replacement),
                queue_tint.in_set(RenderSystems::Queue),
            ),
        );
        let world = app.sub_app_mut(RenderApp).world_mut();
        let device = world.resource::<RenderDevice>().clone();
        world.insert_resource(Tint(tint_pipeline(&device)));
        world.insert_resource(raster::FixturePipelines {
            checker: raster::fixture_pipeline(&device, 1, "checker"),
            sloped_checker: raster::fixture_pipeline(&device, 1, "sloped_checker"),
            silhouette: raster::fixture_pipeline(&device, 1, "silhouette"),
            scene: 0,
        });
    });
    app.world_mut()
        .entity_mut(camera)
        .insert(render_scene::shaderpack::ShaderPackRendering);
    let world = app.sub_app(RenderApp).world();
    let gpu = super::super::pipelines::GpuContext {
        device: world.resource::<RenderDevice>().wgpu_device().clone(),
        queue: (**world.resource::<RenderQueue>()).clone(),
        adapter: (**world.resource::<RenderAdapter>()).clone(),
    };
    let source = shaderpack::ShaderPack::new(shaderpack::PackSource::from_files([
        (
            "gbuffers_terrain.vsh",
            "#version 120\nvoid main() { gl_Position = ftransform(); }",
        ),
        (
            "gbuffers_terrain.fsh",
            "#version 120\nconst vec4 colortex0ClearColor = vec4(0.0, 0.0, 1.0, 1.0); void main() { gl_FragColor = vec4(0.0, 0.0, 1.0, 1.0); }",
        ),
        (
            "final.vsh",
            "#version 120\nvoid main() { gl_Position = ftransform(); }",
        ),
        (
            "final.fsh",
            "#version 120\nuniform sampler2D colortex0; uniform float viewWidth; uniform float viewHeight; void main() { gl_FragColor = texture2D(colortex0, gl_FragCoord.xy / vec2(viewWidth, viewHeight)); }",
        ),
    ]));
    let pack = super::super::loader::build(
        &source,
        "fixture",
        super::super::plugin::JAVA_DIMENSIONS[0],
        &default(),
        None,
        &gpu,
        &default(),
        &default(),
    )
    .unwrap();
    app.world_mut()
        .insert_resource(super::super::plugin::LoadedShaderPack(Some(Arc::new(pack))));
    app.sub_app_mut(RenderApp)
        .world_mut()
        .insert_resource(render_scene::ParticleGpuFrame {
            blend: Arc::from([particles::ParticleInstance::default()]),
            ..default()
        });
    raster::frame(&mut app);
    let pixels = raster::read_pixels(&app, &output);
    if let Some(path) = std::env::var_os("CINNABAR_NATIVE_PACK_CAPTURE") {
        image::save_buffer(
            path,
            &pixels,
            raster::SIDE,
            raster::SIDE,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    for pixel in pixels.chunks_exact(4) {
        assert!(
            (i16::from(pixel[0]) - 188).abs() <= 1
                && pixel[1] == 0
                && (i16::from(pixel[2]) - 188).abs() <= 1,
            "native blend over pack blue: {pixel:?}"
        );
    }
}
