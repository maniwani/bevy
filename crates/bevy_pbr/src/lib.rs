pub mod wireframe;

mod alpha;
mod bundle;
mod light;
mod material;
mod pbr_material;
mod render;

pub use alpha::*;
pub use bundle::*;
pub use light::*;
pub use material::*;
pub use pbr_material::*;
pub use render::*;

pub mod prelude {
    #[doc(hidden)]
    pub use crate::{
        alpha::AlphaMode,
        bundle::{DirectionalLightBundle, MaterialMeshBundle, PbrBundle, PointLightBundle},
        light::{AmbientLight, DirectionalLight, PointLight},
        material::{Material, MaterialPlugin},
        pbr_material::StandardMaterial,
    };
}

pub mod draw_3d_graph {
    pub mod node {
        /// Label for the shadow pass node.
        pub const SHADOW_PASS: &str = "shadow_pass";
    }
}

use bevy_app::prelude::*;
use bevy_asset::{load_internal_asset, Assets, Handle, HandleUntyped};
use bevy_ecs::prelude::*;
use bevy_reflect::TypeUuid;
use bevy_render::{
    prelude::Color,
    render_graph::RenderGraph,
    render_phase::{sort_phase_system, AddRenderCommand, DrawFunctions},
    render_resource::{Shader, SpecializedMeshPipelines},
    view::VisibilitySystems,
    RenderApp, RenderSet,
};
use bevy_transform::TransformSystem;

pub const PBR_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 4805239651767701046);
pub const SHADOW_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 1836745567947005696);

/// Adds physically-based rendering (PBR) pipeline.
#[derive(Default)]
pub struct PbrPlugin;

impl Plugin for PbrPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, PBR_SHADER_HANDLE, "render/pbr.wgsl", Shader::from_wgsl);
        load_internal_asset!(
            app,
            SHADOW_SHADER_HANDLE,
            "render/depth.wgsl",
            Shader::from_wgsl
        );

        app.register_type::<CubemapVisibleEntities>()
            .register_type::<DirectionalLight>()
            .register_type::<PointLight>()
            .add_plugin(MeshRenderPlugin)
            .add_plugin(MaterialPlugin::<StandardMaterial>::default())
            .init_resource::<AmbientLight>()
            .init_resource::<GlobalVisiblePointLights>()
            .init_resource::<DirectionalLightShadowMap>()
            .init_resource::<PointLightShadowMap>()
            .add_system(
                // NOTE: Clusters need to have been added before update_clusters is run so
                // add as an exclusive system
                add_clusters
                    .at_start()
                    .to(CoreSet::PostUpdate)
                    .to(SimulationLightSystems::AddClusters),
            )
            .add_system(
                assign_lights_to_clusters
                    .to(SimulationLightSystems::AssignLightsToClusters)
                    .to(CoreSet::PostUpdate)
                    .after(TransformSystem::TransformPropagate),
            )
            .add_system(
                update_directional_light_frusta
                    .to(SimulationLightSystems::UpdateDirectionalLightFrusta)
                    .to(CoreSet::PostUpdate)
                    .after(TransformSystem::TransformPropagate),
            )
            .add_system(
                update_point_light_frusta
                    .to(SimulationLightSystems::UpdatePointLightFrusta)
                    .to(CoreSet::PostUpdate)
                    .after(TransformSystem::TransformPropagate)
                    .after(SimulationLightSystems::AssignLightsToClusters),
            )
            .add_system(
                check_light_mesh_visibility
                    .to(SimulationLightSystems::CheckLightVisibility)
                    .to(CoreSet::PostUpdate)
                    .after(TransformSystem::TransformPropagate)
                    .after(VisibilitySystems::CalculateBounds)
                    .after(SimulationLightSystems::UpdateDirectionalLightFrusta)
                    .after(SimulationLightSystems::UpdatePointLightFrusta)
                    // NOTE: This MUST be scheduled AFTER the core renderer visibility check
                    // because that resets entity ComputedVisibility for the first view
                    // which would override any results from this otherwise
                    .after(VisibilitySystems::CheckVisibility),
            );

        app.world
            .resource_mut::<Assets<StandardMaterial>>()
            .set_untracked(
                Handle::<StandardMaterial>::default(),
                StandardMaterial {
                    base_color: Color::rgb(1.0, 0.0, 0.5),
                    unlit: true,
                    ..Default::default()
                },
            );

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        render_app
            .add_system(
                render::extract_clusters
                    .to(RenderLightSystems::ExtractClusters)
                    .to(RenderSet::Extract),
            )
            .add_system(
                render::extract_lights
                    .to(RenderLightSystems::ExtractLights)
                    .to(RenderSet::Extract),
            )
            .add_system(
                // this is added as an exclusive system because it contributes new views. it must run (and have Commands applied)
                // _before_ the `prepare_views()` system is run. ideally this becomes a normal system when "stageless" features come out
                render::prepare_lights
                    .at_start()
                    .to(RenderLightSystems::PrepareLights)
                    .to(RenderSet::Prepare),
            )
            .add_system(
                // this is added as an exclusive system because it contributes new views. it must run (and have Commands applied)
                // _before_ the `prepare_views()` system is run. ideally this becomes a normal system when "stageless" features come out
                render::prepare_clusters
                    .at_start()
                    .to(RenderLightSystems::PrepareClusters)
                    .after(RenderLightSystems::PrepareLights)
                    .to(RenderSet::Prepare),
            )
            .add_system(
                render::queue_shadows
                    .to(RenderLightSystems::QueueShadows)
                    .to(RenderSet::Queue),
            )
            .add_system(render::queue_shadow_view_bind_group.to(RenderSet::Queue))
            .add_system(sort_phase_system::<Shadow>.to(RenderSet::PhaseSort))
            .init_resource::<ShadowPipeline>()
            .init_resource::<DrawFunctions<Shadow>>()
            .init_resource::<LightMeta>()
            .init_resource::<GlobalLightMeta>()
            .init_resource::<SpecializedMeshPipelines<ShadowPipeline>>();

        let shadow_pass_node = ShadowPassNode::new(&mut render_app.world);
        render_app.add_render_command::<Shadow, DrawShadowMesh>();
        let mut graph = render_app.world.resource_mut::<RenderGraph>();
        let draw_3d_graph = graph
            .get_sub_graph_mut(bevy_core_pipeline::draw_3d_graph::NAME)
            .unwrap();
        draw_3d_graph.add_node(draw_3d_graph::node::SHADOW_PASS, shadow_pass_node);
        draw_3d_graph
            .add_node_edge(
                draw_3d_graph::node::SHADOW_PASS,
                bevy_core_pipeline::draw_3d_graph::node::MAIN_PASS,
            )
            .unwrap();
        draw_3d_graph
            .add_slot_edge(
                draw_3d_graph.input_node().unwrap().id,
                bevy_core_pipeline::draw_3d_graph::input::VIEW_ENTITY,
                draw_3d_graph::node::SHADOW_PASS,
                ShadowPassNode::IN_VIEW,
            )
            .unwrap();
    }
}
