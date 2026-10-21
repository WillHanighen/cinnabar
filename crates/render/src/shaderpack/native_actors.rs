//! Selects native actor layers independently from replacement terrain.

use bevy::{ecs::system::SystemParam, prelude::*};

#[cfg(feature = "shaderpacks")]
use super::{
    actors::{self, ActorSkinPipelines},
    plugin::ShaderPackViews,
};

/// Reads current pack readiness when native commands execute, including allocation failures.
#[derive(SystemParam)]
pub(crate) struct NativeActorCoverage<'w> {
    #[cfg(feature = "shaderpacks")]
    views: Option<Res<'w, ShaderPackViews>>,
    #[cfg(feature = "shaderpacks")]
    pipelines: Option<Res<'w, ActorSkinPipelines>>,
    #[cfg(feature = "shaderpacks")]
    cache: Option<Res<'w, bevy::render::render_resource::PipelineCache>>,
    #[cfg(feature = "shaderpacks")]
    hand: Option<Res<'w, render_scene::HandRigScene>>,
    #[cfg(feature = "shaderpacks")]
    viewmodel: Option<Res<'w, render_scene::ViewmodelScene>>,
    #[cfg(feature = "shaderpacks")]
    blocks: Option<Res<'w, render_scene::BlockEntityFrameResource>>,
    #[cfg(feature = "shaderpacks")]
    drawn: Option<Res<'w, render_scene::shaderpack::ShaderPackDrawnCategories>>,
    _marker: std::marker::PhantomData<&'w ()>,
}

impl NativeActorCoverage<'_> {
    /// Whether the pack can draw actor geometry for this view this frame.
    pub(crate) fn actors(&self, view: Entity) -> bool {
        #[cfg(feature = "shaderpacks")]
        {
            self.drawn.as_ref().is_some_and(|drawn| drawn.terrain)
                && self
                    .pipelines
                    .as_deref()
                    .zip(self.cache.as_deref())
                    .and_then(|(pipelines, cache)| pipelines.kernels(cache))
                    .is_some()
                && self.views.as_ref().is_some_and(|views| {
                    views
                        .states
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get(&view)
                        .is_some_and(|state| !state.actors.allocation_failed)
                })
        }
        #[cfg(not(feature = "shaderpacks"))]
        {
            let _ = view;
            false
        }
    }

    /// Keeps incompatible block-entity lists on their native material path.
    pub(crate) fn blocks(&self, view: Entity) -> bool {
        #[cfg(feature = "shaderpacks")]
        {
            self.actors(view)
                && actors::compatibility::frame_supported(
                    None,
                    None,
                    self.blocks.as_deref().map(|frame| &frame.0),
                )
        }
        #[cfg(not(feature = "shaderpacks"))]
        {
            let _ = view;
            false
        }
    }

    /// Static viewmodels and incompatible rigs retain the native hand's private depth.
    pub(crate) fn hand(&self, view: Entity) -> bool {
        #[cfg(feature = "shaderpacks")]
        {
            self.actors(view) && hand_supported(self.hand.as_deref(), self.viewmodel.as_deref())
        }
        #[cfg(not(feature = "shaderpacks"))]
        {
            let _ = view;
            false
        }
    }
}

/// Tests hand compatibility without making it a condition of world replacement.
#[cfg(feature = "shaderpacks")]
pub(super) fn hand_supported(
    hand: Option<&render_scene::HandRigScene>,
    viewmodel: Option<&render_scene::ViewmodelScene>,
) -> bool {
    viewmodel.is_none_or(|scene| scene.frame.is_none())
        && actors::compatibility::frame_supported(None, hand, None)
}
