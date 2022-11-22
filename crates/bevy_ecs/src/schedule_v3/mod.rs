mod condition;
mod config;
mod executor;
mod graph_utils;
mod migration;
mod schedule;
mod set;
mod state;

pub use self::condition::*;
pub use self::config::*;
pub use self::executor::*;
use self::graph_utils::*;
pub use self::migration::*;
pub use self::schedule::*;
pub use self::set::*;
pub use self::state::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    pub use crate as bevy_ecs;
    pub use crate::schedule_v3::{IntoSystemConfig, IntoSystemSetConfig, Schedule, SystemSet};
    pub use crate::system::{Res, ResMut};
    pub use crate::{prelude::World, system::Resource};

    #[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
    enum TestSet {
        A,
        B,
        C,
        D,
        X,
    }

    #[derive(Resource, Default)]
    struct SystemOrder(Vec<u32>);

    #[derive(Resource, Default)]
    struct RunConditionBool(pub bool);

    #[derive(Resource, Default)]
    struct Counter(pub AtomicU32);

    fn make_exclusive_system(tag: u32) -> impl FnMut(&mut World) {
        move |world| world.resource_mut::<SystemOrder>().0.push(tag)
    }

    fn make_function_system(tag: u32) -> impl FnMut(ResMut<SystemOrder>) {
        move |mut resource: ResMut<SystemOrder>| resource.0.push(tag)
    }

    fn named_system(mut resource: ResMut<SystemOrder>) {
        resource.0.push(u32::MAX);
    }

    fn named_exclusive_system(world: &mut World) {
        world.resource_mut::<SystemOrder>().0.push(u32::MAX);
    }

    fn counting_system(counter: Res<Counter>) {
        counter.0.fetch_add(1, Ordering::Relaxed);
    }

    mod system_execution {
        use super::*;

        #[test]
        fn run_system() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<SystemOrder>();

            schedule.add_system(make_function_system(0));
            schedule.run(&mut world);

            assert_eq!(world.resource::<SystemOrder>().0, vec![0]);
        }

        #[test]
        fn run_exclusive_system() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<SystemOrder>();

            schedule.add_system(make_exclusive_system(0));
            schedule.run(&mut world);

            assert_eq!(world.resource::<SystemOrder>().0, vec![0]);
        }

        #[test]
        #[cfg(not(miri))]
        fn parallel_execution() {
            use bevy_tasks::{ComputeTaskPool, TaskPool};
            use std::sync::{Arc, Barrier};

            let mut world = World::default();
            let mut schedule = Schedule::default();
            let thread_count = ComputeTaskPool::init(TaskPool::default).thread_num();

            let barrier = Arc::new(Barrier::new(thread_count));

            for _ in 0..thread_count {
                let inner = barrier.clone();
                schedule.add_system(move || {
                    inner.wait();
                });
            }

            schedule.run(&mut world);
        }
    }

    mod system_ordering {
        use super::*;

        #[test]
        fn order_systems() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<SystemOrder>();

            schedule.add_system(named_system);
            schedule.add_system(make_function_system(1).before(named_system));
            schedule.add_system(
                make_function_system(0)
                    .after(named_system)
                    .in_set(TestSet::A),
            );
            schedule.run(&mut world);

            assert_eq!(world.resource::<SystemOrder>().0, vec![1, u32::MAX, 0]);

            world.insert_resource(SystemOrder::default());

            assert_eq!(world.resource::<SystemOrder>().0, vec![]);

            // modify the schedule after it's been initialized and test ordering with sets
            schedule.configure_set(TestSet::A.after(named_system));
            schedule.add_system(
                make_function_system(3)
                    .before(TestSet::A)
                    .after(named_system),
            );
            schedule.add_system(make_function_system(4).after(TestSet::A));
            schedule.run(&mut world);

            assert_eq!(
                world.resource::<SystemOrder>().0,
                vec![1, u32::MAX, 3, 0, 4]
            );
        }

        #[test]
        fn order_exclusive_systems() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<SystemOrder>();

            schedule.add_system(named_exclusive_system);
            schedule.add_system(make_exclusive_system(1).before(named_exclusive_system));
            schedule.add_system(make_exclusive_system(0).after(named_exclusive_system));
            schedule.run(&mut world);

            assert_eq!(world.resource::<SystemOrder>().0, vec![1, u32::MAX, 0]);
        }

        #[test]
        fn add_systems_correct_order() {
            #[derive(Resource)]
            struct X(Vec<TestSet>);

            let mut world = World::new();
            world.init_resource::<SystemOrder>();

            let mut schedule = Schedule::new();
            schedule.add_systems(
                (
                    make_function_system(0),
                    make_function_system(1),
                    make_exclusive_system(2),
                    make_function_system(3),
                )
                    .chain(),
            );

            schedule.run(&mut world);
            assert_eq!(world.resource::<SystemOrder>().0, vec![0, 1, 2, 3]);
        }
    }

    mod conditions {
        use super::*;

        #[test]
        fn system_with_condition() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<RunConditionBool>();
            world.init_resource::<SystemOrder>();

            schedule.add_system(
                make_function_system(0).run_if(|condition: Res<RunConditionBool>| condition.0),
            );

            schedule.run(&mut world);
            assert_eq!(world.resource::<SystemOrder>().0, vec![]);

            world.resource_mut::<RunConditionBool>().0 = true;
            schedule.run(&mut world);
            assert_eq!(world.resource::<SystemOrder>().0, vec![0]);
        }

        #[test]
        fn run_exclusive_system_with_condition() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<RunConditionBool>();
            world.init_resource::<SystemOrder>();

            schedule.add_system(
                make_exclusive_system(0).run_if(|condition: Res<RunConditionBool>| condition.0),
            );

            schedule.run(&mut world);
            assert_eq!(world.resource::<SystemOrder>().0, vec![]);

            world.resource_mut::<RunConditionBool>().0 = true;
            schedule.run(&mut world);
            assert_eq!(world.resource::<SystemOrder>().0, vec![0]);
        }

        #[test]
        fn multiple_conditions_on_system() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<Counter>();

            schedule.add_system(counting_system.run_if(|| false).run_if(|| false));
            schedule.add_system(counting_system.run_if(|| true).run_if(|| false));
            schedule.add_system(counting_system.run_if(|| false).run_if(|| true));
            schedule.add_system(counting_system.run_if(|| true).run_if(|| true));

            schedule.run(&mut world);
            assert_eq!(world.resource::<Counter>().0.load(Ordering::Relaxed), 1);
        }

        #[test]
        fn multiple_conditions_on_system_sets() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<Counter>();

            schedule.configure_set(TestSet::A.run_if(|| false).run_if(|| false));
            schedule.add_system(counting_system.in_set(TestSet::A));
            schedule.configure_set(TestSet::B.run_if(|| true).run_if(|| false));
            schedule.add_system(counting_system.in_set(TestSet::B));
            schedule.configure_set(TestSet::C.run_if(|| false).run_if(|| true));
            schedule.add_system(counting_system.in_set(TestSet::C));
            schedule.configure_set(TestSet::D.run_if(|| true).run_if(|| true));
            schedule.add_system(counting_system.in_set(TestSet::D));

            schedule.run(&mut world);
            assert_eq!(world.resource::<Counter>().0.load(Ordering::Relaxed), 1);
        }

        #[test]
        fn systems_nested_in_system_sets() {
            let mut world = World::default();
            let mut schedule = Schedule::default();

            world.init_resource::<Counter>();

            schedule.configure_set(TestSet::A.run_if(|| false));
            schedule.add_system(counting_system.in_set(TestSet::A).run_if(|| false));
            schedule.configure_set(TestSet::B.run_if(|| true));
            schedule.add_system(counting_system.in_set(TestSet::B).run_if(|| false));
            schedule.configure_set(TestSet::C.run_if(|| false));
            schedule.add_system(counting_system.in_set(TestSet::C).run_if(|| true));
            schedule.configure_set(TestSet::D.run_if(|| true));
            schedule.add_system(counting_system.in_set(TestSet::D).run_if(|| true));

            schedule.run(&mut world);
            assert_eq!(world.resource::<Counter>().0.load(Ordering::Relaxed), 1);
        }
    }

    mod schedule_build_errors {
        use super::*;

        #[test]
        #[should_panic]
        fn dependency_loop() {
            let mut schedule = Schedule::new();
            schedule.configure_set(TestSet::X.after(TestSet::X));
        }

        #[test]
        fn dependency_cycle() {
            let mut world = World::new();
            let mut schedule = Schedule::new();

            schedule.configure_set(TestSet::A.after(TestSet::B));
            schedule.configure_set(TestSet::B.after(TestSet::A));

            let result = schedule.initialize(&mut world);
            assert!(matches!(result, Err(ScheduleBuildError::DependencyCycle)));

            fn foo() {}
            fn bar() {}

            let mut world = World::new();
            let mut schedule = Schedule::new();

            schedule.add_systems((foo.after(bar), bar.after(foo)));
            let result = schedule.initialize(&mut world);
            assert!(matches!(result, Err(ScheduleBuildError::DependencyCycle)));
        }

        #[test]
        #[should_panic]
        fn hierarchy_loop() {
            let mut schedule = Schedule::new();
            schedule.configure_set(TestSet::X.in_set(TestSet::X));
        }

        #[test]
        fn hierarchy_cycle() {
            let mut world = World::new();
            let mut schedule = Schedule::new();

            schedule.configure_set(TestSet::A.in_set(TestSet::B));
            schedule.configure_set(TestSet::B.in_set(TestSet::A));

            let result = schedule.initialize(&mut world);
            assert!(matches!(result, Err(ScheduleBuildError::HierarchyCycle)));
        }

        #[test]
        fn system_type_set_ambiguity() {
            // Define some systems.
            fn foo() {}
            fn bar() {}

            let mut world = World::new();
            let mut schedule = Schedule::new();

            // Schedule `bar` to run after `foo`.
            schedule.add_system(foo);
            schedule.add_system(bar.after(foo));

            // There's only one `foo`, so it's fine.
            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());

            // Schedule another `foo`.
            schedule.add_system(foo);

            // When there are multiple instances of `foo`, dependencies on
            // `foo` are no longer allowed. Too much ambiguity.
            let result = schedule.initialize(&mut world);
            assert!(matches!(
                result,
                Err(ScheduleBuildError::SystemTypeSetAmbiguity(_))
            ));

            // same goes for `ambiguous_with`
            let mut schedule = Schedule::new();
            schedule.add_system(foo);
            schedule.add_system(bar.ambiguous_with(foo));
            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());
            schedule.add_system(foo);
            let result = schedule.initialize(&mut world);
            assert!(matches!(
                result,
                Err(ScheduleBuildError::SystemTypeSetAmbiguity(_))
            ));
        }

        #[test]
        #[should_panic]
        fn in_system_type_set() {
            fn foo() {}
            fn bar() {}

            let mut schedule = Schedule::new();
            schedule.add_system(foo.in_set(bar.into_system_set()));
        }

        #[test]
        #[should_panic]
        fn configure_system_type_set() {
            fn foo() {}
            let mut schedule = Schedule::new();
            schedule.configure_set(foo.into_system_set());
        }

        #[test]
        fn hierarchy_redundancy() {
            let mut world = World::new();
            let mut schedule = Schedule::new();

            schedule.set_build_settings(
                ScheduleBuildSettings::new().with_hierarchy_detection(LogLevel::Error),
            );

            // Add `A`.
            schedule.configure_set(TestSet::A);

            // Add `B` as child of `A`.
            schedule.configure_set(TestSet::B.in_set(TestSet::A));

            // Add `X` as child of both `A` and `B`.
            schedule.configure_set(TestSet::X.in_set(TestSet::A).in_set(TestSet::B));

            // `X` cannot be the `A`'s child and grandchild at the same time.
            let result = schedule.initialize(&mut world);
            assert!(matches!(
                result,
                Err(ScheduleBuildError::HierarchyRedundancy)
            ));
        }

        #[test]
        fn cross_dependency() {
            let mut world = World::new();
            let mut schedule = Schedule::new();

            // Add `B` and give it both kinds of relationships with `A`.
            schedule.configure_set(TestSet::B.in_set(TestSet::A));
            schedule.configure_set(TestSet::B.after(TestSet::A));
            let result = schedule.initialize(&mut world);
            assert!(matches!(
                result,
                Err(ScheduleBuildError::CrossDependency(_, _))
            ));
        }
    }

    mod system_ambiguity_errors {
        use crate::{
            event::{EventReader, EventWriter, Events},
            prelude::{Component, With, Without},
            system::{NonSend, NonSendMut, Query},
        };

        use super::*;

        #[derive(Resource)]
        struct R;

        #[derive(Component)]
        struct A;

        #[derive(Component)]
        struct B;

        // An event type
        struct E;

        fn empty_system() {}
        fn res_system(_res: Res<R>) {}
        fn resmut_system(_res: ResMut<R>) {}
        fn nonsend_system(_ns: NonSend<R>) {}
        fn nonsendmut_system(_ns: NonSendMut<R>) {}
        fn read_component_system(_query: Query<&A>) {}
        fn write_component_system(_query: Query<&mut A>) {}
        fn with_filtered_component_system(_query: Query<&mut A, With<B>>) {}
        fn without_filtered_component_system(_query: Query<&mut A, Without<B>>) {}
        fn event_reader_system(_reader: EventReader<E>) {}
        fn event_writer_system(_writer: EventWriter<E>) {}
        fn event_resource_system(_events: ResMut<Events<E>>) {}
        fn read_world_system(_world: &World) {}
        fn write_world_system(_world: &mut World) {}

        #[test]
        fn one_of_everything() {
            let mut world = World::new();
            world.insert_resource(R);
            world.spawn(A);
            world.init_resource::<Events<E>>();

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((resmut_system, write_component_system, event_writer_system));

            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());
        }

        #[test]
        fn read_only() {
            let mut world = World::new();
            world.insert_resource(R);
            world.spawn(A);
            world.init_resource::<Events<E>>();

            let mut schedule = Schedule::new();
            schedule.add_systems((
                empty_system,
                empty_system,
                res_system,
                res_system,
                nonsend_system,
                nonsend_system,
                read_component_system,
                read_component_system,
                event_reader_system,
                event_reader_system,
                read_world_system,
                read_world_system,
            ));

            schedule.set_build_settings(
                ScheduleBuildSettings::new().with_ambiguity_detection(LogLevel::Error),
            );

            schedule.add_systems((res_system, resmut_system));
            let result = schedule.initialize(&mut world);
            assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
        }

        #[test]
        fn read_world() {
            let mut world = World::new();
            world.insert_resource(R);
            world.spawn(A);
            world.init_resource::<Events<E>>();

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((
                resmut_system,
                write_component_system,
                event_writer_system,
                read_world_system,
            ));

            let result = schedule.initialize(&mut world);
            // ambiguity_count == 3
            assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
        }

        #[test]
        fn resources() {
            let mut world = World::new();
            world.insert_resource(R);

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((resmut_system, res_system));

            let result = schedule.initialize(&mut world);
            // ambiguity_count == 1
            assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
        }

        #[test]
        fn nonsend() {
            let mut world = World::new();
            world.insert_resource(R);

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((nonsendmut_system, nonsend_system));

            let result = schedule.initialize(&mut world);
            // ambiguity_count == 1
            assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
        }

        #[test]
        fn components() {
            let mut world = World::new();
            world.insert_resource(R);

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((read_component_system, write_component_system));

            let result = schedule.initialize(&mut world);
            // ambiguity_count == 1
            assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
        }

        #[test]
        #[ignore = "Known failing but fix is non-trivial: https://github.com/bevyengine/bevy/issues/4381"]
        fn filtered_components() {
            let mut world = World::new();
            world.insert_resource(R);

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((
                with_filtered_component_system,
                without_filtered_component_system,
            ));

            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());
        }

        #[test]
        fn events() {
            let mut world = World::new();
            world.init_resource::<Events<E>>();

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            // All of these systems clash
            schedule.add_systems((
                event_reader_system,
                event_writer_system,
                event_resource_system,
            ));

            let result = schedule.initialize(&mut world);
            // ambiguity_count == 3
            assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
        }

        #[test]
        fn exclusive() {
            let mut world = World::new();
            world.insert_resource(R);
            world.spawn(A);
            world.init_resource::<Events<E>>();

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            // All of these systems clash
            schedule.add_systems((
                // All 3 of these conflict with each other
                write_world_system,
                write_world_system,
                res_system,
            ));

            let result = schedule.initialize(&mut world);
            // ambiguity_count == 3
            assert!(matches!(result, Err(ScheduleBuildError::Ambiguity)));
        }

        // Tests for silencing and resolving ambiguities

        #[test]
        fn before_and_after() {
            let mut world = World::new();
            world.init_resource::<Events<E>>();

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((
                event_reader_system.before(event_writer_system),
                event_writer_system,
                event_resource_system.after(event_writer_system),
            ));

            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());
        }

        // TODO: couldn't figure out if there was analogous functionality for this test
        #[test]
        fn ignore_all_ambiguities() {
            let mut world = World::new();
            world.insert_resource(R);

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((
                resmut_system.ambiguous_with_all(),
                res_system,
                nonsend_system,
            ));

            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());
        }

        #[test]
        fn ambiguous_with_label() {
            let mut world = World::new();
            world.insert_resource(R);

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((
                write_component_system.ambiguous_with(TestSet::X),
                res_system.in_set(TestSet::X),
                nonsend_system.in_set(TestSet::X),
            ));

            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());
        }

        #[test]
        fn ambiguous_with_system() {
            let mut world = World::new();
            world.insert_resource(R);

            let mut schedule = Schedule::new();
            schedule.set_build_settings(
                ScheduleBuildSettings::default().with_ambiguity_detection(LogLevel::Error),
            );
            schedule.add_systems((
                write_component_system.ambiguous_with(read_component_system),
                read_component_system,
            ));

            let result = schedule.initialize(&mut world);
            assert!(result.is_ok());
        }

        // TODO: needs changes to be able to check the output
        // fn system_a(_res: ResMut<R>) {}
        // fn system_b(_res: ResMut<R>) {}
        // fn system_c(_res: ResMut<R>) {}
        // fn system_d(_res: ResMut<R>) {}
        // fn system_e(_res: ResMut<R>) {}

        // // Tests that the correct ambiguities were reported in the correct order.
        // #[test]
        // fn correct_ambiguities() {
        //     use super::*;

        //     let mut world = World::new();
        //     world.insert_resource(R);

        //     let mut test_stage = SystemStage::parallel();
        //     test_stage
        //         .add_system(system_a)
        //         .add_system(system_b)
        //         .add_system(system_c.ignore_all_ambiguities())
        //         .add_system(system_d.ambiguous_with(system_b))
        //         .add_system(system_e.after(system_a));

        //     test_stage.run(&mut world);

        //     let ambiguities = test_stage.ambiguities(&world);
        //     assert_eq!(
        //         ambiguities,
        //         vec![
        //             SystemOrderAmbiguity {
        //                 system_names: [
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_a".to_string(),
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_b".to_string()
        //                 ],
        //                 conflicts: vec![
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::R".to_string()
        //                 ],
        //                 segment: SystemStageSegment::Parallel,
        //             },
        //             SystemOrderAmbiguity {
        //                 system_names: [
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_a".to_string(),
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_d".to_string()
        //                 ],
        //                 conflicts: vec![
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::R".to_string()
        //                 ],
        //                 segment: SystemStageSegment::Parallel,
        //             },
        //             SystemOrderAmbiguity {
        //                 system_names: [
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_b".to_string(),
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_e".to_string()
        //                 ],
        //                 conflicts: vec![
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::R".to_string()
        //                 ],
        //                 segment: SystemStageSegment::Parallel,
        //             },
        //             SystemOrderAmbiguity {
        //                 system_names: [
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_d".to_string(),
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::system_e".to_string()
        //                 ],
        //                 conflicts: vec![
        //                     "bevy_ecs::schedule::ambiguity_detection::tests::R".to_string()
        //                 ],
        //                 segment: SystemStageSegment::Parallel,
        //             },
        //         ]
        //     );
        // }
    }
}
