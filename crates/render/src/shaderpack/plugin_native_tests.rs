//! Native layers must not interrupt a ready pack's replacement queues.

use super::*;
use bevy::ecs::system::RunSystemOnce;

/// Builds an empty, ready terrain pack without native actor kernels or game carriers.
pub(super) fn ready_world() -> World {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::NOOP,
        backend_options: wgpu::BackendOptions {
            noop: wgpu::NoopBackendOptions {
                enable: true,
                ..default()
            },
            ..default()
        },
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = bevy::tasks::block_on(instance.request_adapter(&default())).unwrap();
    let (device, queue) = bevy::tasks::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits {
            max_storage_buffers_per_shader_stage: crate::required_vertex_storage_buffers(),
            ..default()
        },
        ..default()
    }))
    .unwrap();
    let gpu = GpuContext {
        device,
        queue,
        adapter,
    };
    let source = shaderpack::ShaderPack::new(shaderpack::PackSource::from_files([
        (
            "gbuffers_terrain.vsh",
            "#version 120\nvoid main() { gl_Position = ftransform(); }",
        ),
        (
            "gbuffers_terrain.fsh",
            "#version 120\nvoid main() { gl_FragColor = vec4(0.25); }",
        ),
    ]));
    let pack = Arc::new(
        super::super::loader::build(
            &source,
            "fixture",
            JAVA_DIMENSIONS[0],
            &default(),
            None,
            &gpu,
            &default(),
            &default(),
        )
        .unwrap(),
    );
    let state = ViewState::new(&gpu.device, Arc::clone(&pack), [4, 4]).unwrap();
    assert!(state.geometry_targets_ready());
    let mut world = World::new();
    let entity = world.spawn(ShaderPackRendering).id();
    let mut views = ShaderPackViews::default();
    views.states.get_mut().unwrap().insert(entity, state);
    world.insert_resource(views);
    world.insert_resource(LoadedShaderPack(Some(pack)));
    let geometry = ShaderpackGeometry::ready_fixture(&RenderDevice::from(gpu.device));
    assert!(geometry.supported() && geometry.all_sections_fit() && geometry.origin().is_some());
    world.insert_resource(geometry);
    world.init_resource::<ShaderPackEffects>();
    world.init_resource::<ShaderPackDrawnCategories>();
    world
}

/// Runs the production queue decision and checks every replacement category.
pub(super) fn assert_replacement(world: &mut World) {
    world.run_system_once(configure_replacement).unwrap();
    assert_eq!(
        *world.resource::<ShaderPackDrawnCategories>(),
        ShaderPackDrawnCategories {
            terrain: true,
            actors: true,
            block_entities: true,
            hand: true,
        }
    );
}

#[test]
fn pending_actor_kernels_leave_pack_categories_drawn() {
    assert_replacement(&mut ready_world());
}

#[test]
fn uncovered_particles_leave_pack_categories_drawn() {
    let mut world = ready_world();
    for mode in 0..3 {
        let mut frame = render_scene::ParticleGpuFrame::default();
        let instances = Arc::from([particles::ParticleInstance::default()]);
        match mode {
            0 => frame.opaque = instances,
            1 => frame.blend = instances,
            _ => frame.add = instances,
        }
        world.insert_resource(frame);
        assert_replacement(&mut world);
    }
}

#[test]
fn uncovered_weather_leaves_pack_categories_drawn() {
    let mut world = ready_world();
    world.insert_resource(render_scene::PrecipitationScene {
        layers: vec![render_model::PrecipitationLayerRecord {
            particle_count: 1,
            ..default()
        }],
        ..default()
    });
    assert_replacement(&mut world);
}

#[test]
fn screen_overlay_leaves_pack_categories_drawn() {
    let mut world = ready_world();
    world.insert_resource(render_scene::ScreenOverlayScene {
        layers: vec![render_scene::ScreenOverlayLayer {
            kind: render_scene::ScreenOverlayKind::SpyglassScope,
            rgb: [1.0; 3],
            alpha: 1.0,
        }],
        ..default()
    });
    assert_replacement(&mut world);
}

#[test]
fn unsupported_actor_leaves_pack_categories_drawn() {
    let mut world = ready_world();
    let mut frame = render_scene::ActorRenderFrame::default();
    frame.rig.instances = Arc::from([render_model::ActorGpuInstance {
        material: render_model::ACTOR_COLOR_MASK_MATERIAL_FLAG,
        ..default()
    }]);
    assert!(!actors::compatibility::frame_supported(
        Some(&frame),
        None,
        None
    ));
    world.insert_resource(frame);
    assert_replacement(&mut world);
}

#[test]
fn uncovered_sky_leaves_pack_categories_drawn() {
    let mut world = ready_world();
    world.init_resource::<render_scene::AtmosphereFrame>();
    assert_replacement(&mut world);
}

#[test]
fn actor_allocation_failure_leaves_pack_terrain_ready() {
    let mut world = ready_world();
    for state in world
        .resource_mut::<ShaderPackViews>()
        .states
        .get_mut()
        .unwrap()
        .values_mut()
    {
        state.actors.allocation_failed = true;
    }
    assert_replacement(&mut world);
}

#[test]
fn missing_geometry_keeps_native_queues_available() {
    let mut world = ready_world();
    world.remove_resource::<ShaderpackGeometry>();
    world.run_system_once(configure_replacement).unwrap();
    assert_eq!(
        *world.resource::<ShaderPackDrawnCategories>(),
        ShaderPackDrawnCategories::default()
    );
}
