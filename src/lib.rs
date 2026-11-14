#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(missing_docs)]

extern crate self as avilex;

#[path = "app/mod.rs"]
mod app_impl;
mod commands;
mod component;
mod dyn_traits;
mod entity;
mod events;
mod query;
mod reactivity;
mod resources;
mod schedule;
mod states;
mod system;
mod world;

/// Re-exports the procedural derive macros for Avilex traits (such as [`Component`], [`Resource`],
/// [`Event`], [`States`], [`QueryData`], [`QueryFilter`], [`SystemParam`], [`SystemSet`], and
/// [`ScheduleLabel`]).
///
/// [`Component`]: crate::ecs::component::Component
/// [`Resource`]: crate::ecs::resources::Resource
/// [`Event`]: crate::ecs::events::Event
/// [`States`]: crate::app::states::States
/// [`QueryData`]: crate::ecs::query::QueryData
/// [`QueryFilter`]: crate::ecs::query::filter::QueryFilter
/// [`SystemParam`]: crate::app::system::SystemParam
/// [`SystemSet`]: crate::app::system::system_set::SystemSet
/// [`ScheduleLabel`]: crate::app::schedule::ScheduleLabel
pub mod derive {
    pub use avilex_macros::{
        Component, ComponentBundle, Event, ParallelSystemParam, QueryData, QueryFilter, Resource,
        ScheduleLabel, States, SystemParam, SystemSet,
    };
}

pub use rayon;

#[doc(hidden)]
pub mod __private {
    pub use indexmap;
    pub use rustc_hash;
}

/// Re-exports the most commonly used Avilex types, traits, macros, and Rayon parallel traits for
/// convenient wildcard importing (`use avilex::prelude::*;`).
pub mod prelude {
    pub use crate::app::{
        App,
        plugin::{Plugin, PluginGroup, PluginGroupBuilder},
        schedule::{First, Last, PostUpdate, PreUpdate, Schedule, ScheduleLabel, Startup, Update},
        states::{NextState, State, States, entered_state, exited_state, in_state},
        system::{
            FunctionIdentity, IntoSystem, System, SystemAccessMeta, SystemIdentity, SystemInput,
            SystemNode, SystemParam,
            condition::{Condition, ConditionFn, RunConditionsList, not, resource_exists},
            system_configs_traits::{ConfigureSystemConfigs, IntoSystemConfigs, SystemConfigs},
            system_error::SystemErrorHandlerFn,
            system_set::{
                IntoSystemSetConfigs, SetConfig, SystemSet, SystemSetConfigs, SystemSetConfigsExt,
                SystemSetExt,
            },
            system_traits::{AsSchedulingTarget, CollectSystemEdgeIds},
        },
    };
    pub use crate::derive::{
        Component, ComponentBundle, Event, ParallelSystemParam, QueryData, QueryFilter, Resource,
        ScheduleLabel, States, SystemParam, SystemSet,
    };
    pub use crate::ecs::{
        commands::{Command, Commands, EntityCommands, ParallelCommands},
        component::{
            Component, ComponentBundle, ComponentMut, ComponentRef,
            hooks::{ComponentHook, ComponentHooksRegisterer, ExclusiveWorld},
        },
        entity::{Entity, EntityMut, EntityRef},
        events::{Event, EventReader, EventWriter, ParallelEventReader, ParallelEventWriter},
        query::{
            Has, ParallelQueryAccessor, Query, QueryData, QueryMutableChunk, QueryReadOnlyChunk,
            filter::{Not, Or, QueryFilter, With, Without},
            single::{QuerySingleError, Single, SingleMut},
        },
        reactivity::{
            added::{Added, AddedState, AddedTracker},
            changed::{Changed, ChangedRef, ChangedRefMut, ChangedState, ChangedTracker, Mut},
            removed::{RemovedComponents, RemovedComponentsBuffer},
        },
        resources::{NonSendRes, NonSendResMut, ParallelResourceAccessor, Res, ResMut, Resource},
        world::World,
    };

    pub use crate::rayon::prelude::*;
}

/// Core Entity-Component-System (ECS) primitives, storage, queries, and reactivity.
pub mod ecs {
    /// Component definitions, bundles, RAII guards, and lifecycle hooks.
    pub mod component {
        pub use crate::component::{
            Component, ComponentMut, ComponentRef, bundle::ComponentBundle,
        };

        /// Lifecycle hook callbacks (`on_add`, `on_insert`, `on_discard`, `on_remove`) and
        /// [`ExclusiveWorld`] context.
        ///
        /// [`ExclusiveWorld`]: crate::ecs::component::hooks::ExclusiveWorld
        pub mod hooks {
            pub use crate::component::hooks::{
                ComponentHook, ComponentHooksRegisterer, ExclusiveWorld,
            };
        }
    }

    /// Double-buffered event queues, system parameters, and parallel event handles.
    pub mod events {
        pub use crate::events::{
            Event, EventReader, EventWriter, ParallelEventReader, ParallelEventWriter,
        };
    }

    /// Global singleton resources, system parameter guards, and thread-safe resource accessors.
    pub mod resources {
        pub use crate::resources::{
            NonSendRes, NonSendResMut, ParallelResourceAccessor, Res, ResMut, Resource,
        };
    }

    /// Archetype queries, parallel chunk iterators, and out-of-band query accessors.
    pub mod query {
        pub use crate::query::{
            Has, MatchedArchetypeData, Query, QueryData, QueryMutableChunk, QueryReadOnlyChunk,
        };

        /// Composable query filter predicates ([`With`], [`Without`], [`Or`], and [`Not`]).
        ///
        /// [`With`]: crate::ecs::query::filter::With
        /// [`Without`]: crate::ecs::query::filter::Without
        /// [`Or`]: crate::ecs::query::filter::Or
        /// [`Not`]: crate::ecs::query::filter::Not
        pub mod filter {
            pub use crate::query::filter::{Not, Or, QueryFilter, With, Without};
        }

        /// System parameters and error types for queries expecting a single unique entity.
        pub mod single {
            pub use crate::query::single::{QuerySingleError, Single, SingleMut};
        }

        pub use crate::query::parallel_query::ParallelQueryAccessor;
    }

    /// Double-buffered component addition, mutation, and removal tracking.
    ///
    /// # Component Reactivity Architecture
    ///
    /// avilex uses a demand-driven model to minimize runtime overhead with reactive trackers.
    ///
    /// * **Lazy Tracking Allocations:** Double-buffered tracking queues are not allocated for every component
    ///   type by default. They are registered and initialized only if a system explicitly requests them
    ///   (e.g., via [`RemovedComponents<T>`], [`Changed<T>`], [`ChangedRefMut<T>`], or [`Added<T>`])
    ///   or a parallel handle (e.g., [`ParallelQueryAccessor`]) requests it in its parameters before [`app.build()`],
    ///   or they are manually registered through [`World`] or [`App`].
    /// * **Optimized Runtime Pathways:** Component types that are never used in reactive query filters skip
    ///   frame-boundary memory swaps and tracking management entirely, keeping untracked data paths completely
    ///   unburdened by default.
    /// * **Automated Static Discovery:** During the conversion into systems before finalizing the [`App`], the
    ///   engine automatically analyzes all system parameters, parameters inside [`ParallelQueryAccessor`]
    ///   (when registered before [`app.build()`]), and [`RemovedComponents<T>`] parameters. It
    ///   then auto-registers all necessary trackers by itself without requiring any manual initialization.
    ///
    /// For any tracking data requirements introduced dynamically post-build, explicit tracker registerers
    /// are provided on both [`App`] and [`World`] to be called before [`app.build()`].
    ///
    /// [`App`]: crate::app::App
    /// [`World`]: crate::ecs::world::World
    /// [`RemovedComponents<T>`]: crate::ecs::reactivity::removed::RemovedComponents
    /// [`Changed<T>`]: crate::ecs::reactivity::changed::Changed
    /// [`ChangedRefMut<T>`]: crate::ecs::reactivity::changed::ChangedRefMut
    /// [`Added<T>`]: crate::ecs::reactivity::added::Added
    /// [`ParallelQueryAccessor`]: crate::ecs::query::ParallelQueryAccessor
    /// [`app.build()`]: crate::app::App::build
    pub mod reactivity {
        /// Change detection filters ([`Changed`]), trackers ([`ChangedTracker`]), and smart
        /// pointers ([`Mut`]).
        ///
        /// [`Changed`]: crate::ecs::reactivity::changed::Changed
        /// [`ChangedTracker`]: crate::ecs::reactivity::changed::ChangedTracker
        /// [`Mut`]: crate::ecs::reactivity::changed::Mut
        pub mod changed {
            pub use crate::reactivity::changed::{
                Changed, ChangedRef, ChangedRefMut, ChangedState, ChangedTracker, Mut,
            };
        }

        /// Component addition detection filters ([`Added`]), trackers ([`AddedTracker`]), and
        /// double-buffered state ([`AddedState`]).
        ///
        /// [`Added`]: crate::ecs::reactivity::added::Added
        /// [`AddedTracker`]: crate::ecs::reactivity::added::AddedTracker
        /// [`AddedState`]: crate::ecs::reactivity::added::AddedState
        pub mod added {
            pub use crate::reactivity::added::{Added, AddedState, AddedTracker};
        }

        /// Component removal detection system parameters ([`RemovedComponents`]) and buffers
        /// ([`RemovedComponentsBuffer`]).
        ///
        /// [`RemovedComponents`]: crate::ecs::reactivity::removed::RemovedComponents
        /// [`RemovedComponentsBuffer`]: crate::ecs::reactivity::removed::RemovedComponentsBuffer
        pub mod removed {
            pub use crate::reactivity::removed::{RemovedComponents, RemovedComponentsBuffer};
        }
    }

    /// Deferred structural world mutation commands and command buffers.
    pub mod commands {
        pub use crate::commands::{Command, Commands, EntityCommands, ParallelCommands};
    }

    /// Central ECS [`World`] storage and archetype table internals.
    ///
    /// [`World`]: crate::ecs::world::World
    pub mod world {
        pub use crate::world::World;

        /// Column-oriented [`Archetype`] tables and column borrow guards.
        ///
        /// [`Archetype`]: crate::ecs::world::archetypes::Archetype
        pub mod archetypes {
            pub use crate::world::archetypes::{
                Archetype, ComponentColumn, ComponentColumnRead, ComponentColumnWrite, EntitiesRead,
            };
        }
    }

    /// Generationless 32-bit [`Entity`] identifiers and single-entity views ([`EntityRef`],
    /// [`EntityMut`]).
    ///
    /// [`Entity`]: crate::ecs::entity::Entity
    /// [`EntityRef`]: crate::ecs::entity::EntityRef
    /// [`EntityMut`]: crate::ecs::entity::EntityMut
    pub mod entity {
        pub use crate::entity::{Entity, EntityMut, EntityRef};
    }
}

/// Application builder, scheduling, systems, plugins, and state machine abstractions.
pub mod app {
    pub use crate::app_impl::App;

    /// Executable [`System`] traits, [`SystemParam`] extraction, scheduling metadata, and local
    /// system storage.
    ///
    /// [`System`]: crate::app::system::System
    /// [`SystemParam`]: crate::app::system::SystemParam
    pub mod system {
        pub use crate::system::{
            AccessHashSet, AccessVec, FunctionIdentity, FunctionSystem, IntoSystem,
            ParallelSystemParam, System, SystemAccessMeta, SystemIdentity, SystemInput, SystemNode,
            SystemParam,
            system_storage::{SystemData, SystemExt},
        };

        /// Runtime evaluation predicates ([`Condition`]) and built-in combinators ([`not`],
        /// [`resource_exists`]) for gating system execution.
        ///
        /// [`Condition`]: crate::app::system::condition::Condition
        /// [`not`]: crate::app::system::condition::not
        /// [`resource_exists`]: crate::app::system::condition::resource_exists
        pub mod condition {
            pub use crate::system::condition::{
                Condition, ConditionFn, RunConditionsList, not, resource_exists,
            };
        }

        /// Internal and marker traits for resolving system scheduling targets.
        pub mod system_traits {
            pub use crate::system::system_traits::{
                AsSchedulingTarget, CollectSystemEdgeIds, FunctionTargetMarker,
            };
        }

        /// Configuration traits ([`ConfigureSystemConfigs`], [`IntoSystemConfigs`]) and containers
        /// ([`SystemConfigs`]) for chaining, ordering, and gating systems.
        ///
        /// [`ConfigureSystemConfigs`]: crate::app::system::system_configs_traits::ConfigureSystemConfigs
        /// [`IntoSystemConfigs`]: crate::app::system::system_configs_traits::IntoSystemConfigs
        /// [`SystemConfigs`]: crate::app::system::system_configs_traits::SystemConfigs
        pub mod system_configs_traits {
            pub use crate::system::system_configs_traits::{
                ConfigureSystemConfigs, IntoSystemConfigs, SystemConfigs,
            };
        }

        /// System grouping markers ([`SystemSet`]) and set-level ordering and condition traits.
        ///
        /// [`SystemSet`]: crate::app::system::system_set::SystemSet
        pub mod system_set {
            pub use crate::system::{
                system_set::{GuardIdentity, GuardNode, SystemSet},
                system_set_traits::{
                    IntoSystemSetConfig, IntoSystemSetConfigs, SetConfig, SystemSetConfigs,
                    SystemSetConfigsExt, SystemSetExt,
                },
            };
        }

        /// System error types ([`RunSystemError`], [`SystemParamError`], [`SystemFailError`]) and
        /// global error handling callbacks.
        ///
        /// [`RunSystemError`]: crate::app::system::system_error::RunSystemError
        /// [`SystemParamError`]: crate::app::system::system_error::SystemParamError
        /// [`SystemFailError`]: crate::app::system::system_error::SystemFailError
        pub mod system_error {
            pub use crate::system::system_error::{
                RunSystemError, Severity, SystemErrorHandler, SystemErrorHandlerFn,
                SystemFailError, SystemParamError, default_system_error_handler,
            };
        }
    }

    /// Core scheduling systems, executors, and stage labels.
    ///
    /// # Default Schedule Execution Order
    /// When initializing an application with `App::new()`, the schedules execute in the following precise sequence:
    ///
    /// * **[`Startup`]** (Runs once)
    /// * **[`First`]**
    /// * **[`PreUpdate`]**
    /// * **[`Update`]**
    /// * **[`PostUpdate`]**
    /// * **[`Last`]**
    ///
    /// [`Startup`]: crate::app::schedule::Startup
    /// [`First`]: crate::app::schedule::First
    /// [`PreUpdate`]: crate::app::schedule::PreUpdate
    /// [`Update`]: crate::app::schedule::Update
    /// [`PostUpdate`]: crate::app::schedule::PostUpdate
    /// [`Last`]: crate::app::schedule::Last
    pub mod schedule {
        pub use crate::schedule::{
            DefaultSchedulesPlugin, First, Last, PostUpdate, PreUpdate, Schedule, ScheduleLabel,
            Startup, SystemExecutor, SystemsSchedule, Update,
            executors::{
                multi_threaded::MultiThreadedExecutor, single_threaded::SingleThreadedExecutor,
            },
        };
    }

    /// Object-safe dynamic equality helper traits ([`DynEq`]).
    ///
    /// [`DynEq`]: crate::app::dyn_traits::DynEq
    pub mod dyn_traits {
        pub use crate::dyn_traits::DynEq;
    }

    /// Modular application configuration traits ([`Plugin`], [`PluginGroup`]) and
    /// [`PluginGroupBuilder`].
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`PluginGroup`]: crate::app::plugin::PluginGroup
    /// [`PluginGroupBuilder`]: crate::app::plugin::PluginGroupBuilder
    pub mod plugin {
        pub use crate::app_impl::plugin::{Plugin, PluginGroup, PluginGroupBuilder};
    }

    /// Core state management primitives and transition filters.
    ///
    /// # State Transitions
    /// State transitions are executed during [`World::start_frame_sync`], before any schedules have run.
    ///
    /// [`World::start_frame_sync`]: crate::ecs::world::World::start_frame_sync
    pub mod states {
        pub use crate::states::{NextState, State, States, entered_state, exited_state, in_state};
    }
}
