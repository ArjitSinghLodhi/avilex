use crate::app::App;
use std::any::{TypeId, type_name};
use std::collections::HashSet;

enum PluginError {
    DuplicateUniquePlugin { name: &'static str },
    PluginAlreadyInitialized { name: &'static str },
    PluginDoesNotExist { name: &'static str },
    TargetPluginNotFound { target: &'static str },
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateUniquePlugin { name } => {
                write!(
                    f,
                    "DuplicateUniquePlugin\n\
                    Name: '{}'",
                    name
                )
            }
            Self::PluginAlreadyInitialized { name } => {
                write!(
                    f,
                    "PluginAlreadyInitialized\n\
                    Name: '{}'",
                    name
                )
            }
            Self::PluginDoesNotExist { name } => {
                write!(
                    f,
                    "PluginDoesNotExist\n\
                    Name: '{}'",
                    name
                )
            }
            Self::TargetPluginNotFound { target } => {
                write!(
                    f,
                    "TargetPluginNotFound\n\
                    Target: '{}'",
                    target
                )
            }
        }
    }
}
/// A modular unit of application logic that configures an [`App`] (such as registering systems,
/// resources, events, schedules, or other plugins).
///
/// Any `Fn(&mut App) + 'static` closure automatically implements `Plugin`.
///
/// [`App`]: crate::app::App
pub trait Plugin: 'static {
    /// Configures `app` when the plugin is added via [`App::add_plugins`].
    ///
    /// [`App`]: crate::app::App
    /// [`App::add_plugins`]: crate::app::App::add_plugins
    fn build(&self, app: &mut App);

    /// Returns a human-readable identifier for this plugin, used in diagnostics and duplicate
    /// registration panics.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Returns `true` if adding this plugin more than once to the same [`App`] should panic.
    ///
    /// Override and return `false` to allow multiple instances of this plugin to be registered.
    ///
    /// [`App`]: crate::app::App
    fn is_unique(&self) -> bool {
        true
    }
}

impl<F> Plugin for F
where
    F: Fn(&mut App) + 'static,
{
    fn build(&self, app: &mut App) {
        (self)(app);
    }
}

/// A collection of [`Plugin`]s that can be configured and added together via [`App::add_plugins`].
///
/// [`Plugin`]: crate::app::plugin::Plugin
/// [`App::add_plugins`]: crate::app::App::add_plugins
pub trait PluginGroup: Sized {
    /// Consumes this group and constructs a [`PluginGroupBuilder`] containing its default plugins.
    ///
    /// [`PluginGroupBuilder`]: crate::app::plugin::PluginGroupBuilder
    fn build(self) -> PluginGroupBuilder;

    /// Returns a human-readable name for this plugin group.
    fn name() -> String {
        type_name::<Self>().to_string()
    }

    /// Builds the group and replaces the existing plugin of type `T` with `plugin` if present.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`PluginGroupBuilder`]: crate::app::plugin::PluginGroupBuilder
    fn set<T: Plugin>(self, plugin: T) -> PluginGroupBuilder {
        self.build().set(plugin)
    }

    /// Builds the group and disables the plugin of type `T`.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`App`]: crate::app::App
    /// [`PluginGroupBuilder`]: crate::app::plugin::PluginGroupBuilder
    fn disable<T: Plugin>(self) -> PluginGroupBuilder {
        self.build().disable::<T>()
    }

    /// Builds the group and enables the plugin of type `T`.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`PluginGroupBuilder`]: crate::app::plugin::PluginGroupBuilder
    fn enable<T: Plugin>(self) -> PluginGroupBuilder {
        self.build().enable::<T>()
    }
}

impl<P: Plugin> PluginGroup for P {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new_internal().add(self)
    }
}

impl PluginGroup for PluginGroupBuilder {
    fn build(self) -> PluginGroupBuilder {
        self
    }
}

struct PluginEntry {
    id: TypeId,
    plugin: Box<dyn Plugin>,
}

/// Builder used by [`PluginGroup`] implementations to assemble, order, replace, or toggle
/// individual [`Plugin`] entries before adding them to an [`App`].
///
/// [`PluginGroup`]: crate::app::plugin::PluginGroup
/// [`Plugin`]: crate::app::plugin::Plugin
/// [`App`]: crate::app::App
pub struct PluginGroupBuilder {
    entries: Vec<PluginEntry>,
    disabled: HashSet<TypeId>,
}

impl std::fmt::Debug for PluginGroupBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PluginGroupBuilder")
    }
}

impl PluginGroupBuilder {
    fn new_internal() -> Self {
        Self {
            entries: Vec::new(),
            disabled: HashSet::new(),
        }
    }

    /// Creates an empty [`PluginGroupBuilder`] for building a [`PluginGroup`] `PG`.
    ///
    /// [`PluginGroupBuilder`]: Self
    /// [`PluginGroup`]: crate::app::plugin::PluginGroup
    pub fn start<PG: PluginGroup>() -> Self {
        Self::new_internal()
    }

    /// Returns `true` if a [`Plugin`] of type `T` has been added to this builder (even if
    /// disabled).
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    pub fn contains<T: Plugin>(&self) -> bool {
        let target_id = TypeId::of::<T>();
        self.entries.iter().any(|e| e.id == target_id)
    }

    /// Returns `true` if a [`Plugin`] of type `T` is present in this builder and has not been
    /// disabled.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    pub fn enabled<T: Plugin>(&self) -> bool {
        let target_id = TypeId::of::<T>();
        self.contains::<T>() && !self.disabled.contains(&target_id)
    }

    /// Appends `plugin` to the end of the group.
    ///
    /// # Panics
    /// Panics if [`Plugin::is_unique`] returns `true` for `T` and a plugin of type `T` is already
    /// in the group.
    ///
    /// [`Plugin::is_unique`]: crate::app::plugin::Plugin::is_unique
    #[allow(clippy::should_implement_trait)]
    pub fn add<T: Plugin>(mut self, plugin: T) -> Self {
        let id = TypeId::of::<T>();

        if plugin.is_unique() {
            if self.contains::<T>() {
                panic!(
                    "{}",
                    PluginError::DuplicateUniquePlugin {
                        name: plugin.name()
                    }
                );
            }
            self.entries.retain(|e| e.id != id);
        }

        self.entries.push(PluginEntry {
            id,
            plugin: Box::new(plugin),
        });
        self
    }

    /// Appends `plugin` to the end of the group if no plugin of type `T` is already present.
    ///
    /// Returns `Err((self, plugin))` without modifying the builder if `T` is already registered.
    pub fn try_add<T: Plugin>(self, plugin: T) -> Result<Self, (Self, T)> {
        if self.contains::<T>() {
            return Err((self, plugin));
        }
        Ok(self.add(plugin))
    }

    /// Replaces the existing [`Plugin`] of type `T` in-place with `plugin`, preserving its
    /// position in the group.
    ///
    /// # Panics
    /// Panics if no plugin of type `T` exists in this builder. Use [`PluginGroupBuilder::try_set`]
    /// for a non-panicking alternative.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`PluginGroupBuilder::try_set`]: Self::try_set
    pub fn set<T: Plugin>(mut self, plugin: T) -> Self {
        let id = TypeId::of::<T>();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.plugin = Box::new(plugin);
            self
        } else {
            panic!(
                "{}",
                PluginError::PluginDoesNotExist {
                    name: std::any::type_name::<T>()
                }
            );
        }
    }

    /// Replaces the existing [`Plugin`] of type `T` in-place with `plugin`, preserving its
    /// position in the group.
    ///
    /// Returns `Err((self, plugin))` if no plugin of type `T` exists in this builder.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    pub fn try_set<T: Plugin>(mut self, plugin: T) -> Result<Self, (Self, T)> {
        let id = TypeId::of::<T>();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.plugin = Box::new(plugin);
            Ok(self)
        } else {
            Err((self, plugin))
        }
    }

    /// Inserts `plugin` immediately before the existing `Target` plugin in the group.
    ///
    /// # Panics
    /// Panics if [`Plugin::is_unique`] returns `true` for `Insert` and `Insert` is already in
    /// the group, or if `Target` is not found in the group.
    ///
    /// [`Plugin::is_unique`]: crate::app::plugin::Plugin::is_unique
    pub fn add_before<Target: Plugin, Insert: Plugin>(mut self, plugin: Insert) -> Self {
        let target_id = TypeId::of::<Target>();
        let insert_id = TypeId::of::<Insert>();

        if plugin.is_unique() && self.contains::<Insert>() {
            panic!(
                "{}",
                PluginError::DuplicateUniquePlugin {
                    name: plugin.name()
                }
            );
        }

        self.entries.retain(|e| e.id != insert_id);

        if let Some(pos) = self.entries.iter().position(|e| e.id == target_id) {
            self.entries.insert(
                pos,
                PluginEntry {
                    id: insert_id,
                    plugin: Box::new(plugin),
                },
            );
            self
        } else {
            panic!(
                "{}",
                PluginError::TargetPluginNotFound {
                    target: std::any::type_name::<Target>()
                }
            );
        }
    }

    /// Inserts `plugin` immediately before `Target` if `Target` exists and `Insert` is not already
    /// present in the group.
    ///
    /// Returns `Err((self, plugin))` without modifying the builder if `Target` is missing or
    /// `Insert` is already present.
    pub fn try_add_before<Target: Plugin, Insert: Plugin>(
        self,
        plugin: Insert,
    ) -> Result<Self, (Self, Insert)> {
        if self.contains::<Insert>() || !self.contains::<Target>() {
            return Err((self, plugin));
        }
        Ok(self.add_before::<Target, Insert>(plugin))
    }

    /// Inserts `plugin` immediately before `Target`, replacing any existing instance of `Insert` in
    /// the group.
    ///
    /// Returns `Err((self, plugin))` without modifying the builder if `Target` is not found.
    pub fn try_add_before_overwrite<Target: Plugin, Insert: Plugin>(
        mut self,
        plugin: Insert,
    ) -> Result<Self, (Self, Insert)> {
        if !self.contains::<Target>() {
            return Err((self, plugin));
        }
        let target_id = TypeId::of::<Target>();
        let insert_id = TypeId::of::<Insert>();

        self.entries.retain(|e| e.id != insert_id);

        if let Some(pos) = self.entries.iter().position(|e| e.id == target_id) {
            self.entries.insert(
                pos,
                PluginEntry {
                    id: insert_id,
                    plugin: Box::new(plugin),
                },
            );
        }
        Ok(self)
    }

    /// Inserts `plugin` immediately after the existing `Target` plugin in the group.
    ///
    /// # Panics
    /// Panics if [`Plugin::is_unique`] returns `true` for `Insert` and `Insert` is already in
    /// the group, or if `Target` is not found in the group.
    ///
    /// [`Plugin::is_unique`]: crate::app::plugin::Plugin::is_unique
    pub fn add_after<Target: Plugin, Insert: Plugin>(mut self, plugin: Insert) -> Self {
        let target_id = TypeId::of::<Target>();
        let insert_id = TypeId::of::<Insert>();

        if plugin.is_unique() && self.contains::<Insert>() {
            panic!(
                "{}",
                PluginError::DuplicateUniquePlugin {
                    name: plugin.name()
                }
            );
        }

        self.entries.retain(|e| e.id != insert_id);

        if let Some(pos) = self.entries.iter().position(|e| e.id == target_id) {
            self.entries.insert(
                pos + 1,
                PluginEntry {
                    id: insert_id,
                    plugin: Box::new(plugin),
                },
            );
            self
        } else {
            panic!(
                "{}",
                PluginError::TargetPluginNotFound {
                    target: std::any::type_name::<Target>()
                }
            );
        }
    }

    /// Inserts `plugin` immediately after `Target` if `Target` exists and `Insert` is not already
    /// present in the group.
    ///
    /// Returns `Err((self, plugin))` without modifying the builder if `Target` is missing or
    /// `Insert` is already present.
    pub fn try_add_after<Target: Plugin, Insert: Plugin>(
        self,
        plugin: Insert,
    ) -> Result<Self, (Self, Insert)> {
        if self.contains::<Insert>() || !self.contains::<Target>() {
            return Err((self, plugin));
        }
        Ok(self.add_after::<Target, Insert>(plugin))
    }

    /// Inserts `plugin` immediately after `Target`, replacing any existing instance of `Insert` in
    /// the group.
    ///
    /// Returns `Err((self, plugin))` without modifying the builder if `Target` is not found.
    pub fn try_add_after_overwrite<Target: Plugin, Insert: Plugin>(
        mut self,
        plugin: Insert,
    ) -> Result<Self, (Self, Insert)> {
        if !self.contains::<Target>() {
            return Err((self, plugin));
        }
        let target_id = TypeId::of::<Target>();
        let insert_id = TypeId::of::<Insert>();

        self.entries.retain(|e| e.id != insert_id);

        if let Some(pos) = self.entries.iter().position(|e| e.id == target_id) {
            self.entries.insert(
                pos + 1,
                PluginEntry {
                    id: insert_id,
                    plugin: Box::new(plugin),
                },
            );
        }
        Ok(self)
    }

    /// Marks the [`Plugin`] of type `T` as disabled so it will be skipped during
    /// [`PluginGroupBuilder::finish`].
    ///
    /// # Panics
    /// Panics if no plugin of type `T` exists in this builder.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`PluginGroupBuilder::finish`]: Self::finish
    pub fn disable<T: Plugin>(mut self) -> Self {
        if !self.contains::<T>() {
            panic!(
                "{}",
                PluginError::PluginDoesNotExist {
                    name: std::any::type_name::<T>()
                }
            );
        }
        self.disabled.insert(TypeId::of::<T>());
        self
    }

    /// Re-enables a previously disabled [`Plugin`] of type `T`.
    ///
    /// # Panics
    /// Panics if no plugin of type `T` exists in this builder.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    pub fn enable<T: Plugin>(mut self) -> Self {
        if !self.contains::<T>() {
            panic!(
                "{}",
                PluginError::PluginDoesNotExist {
                    name: std::any::type_name::<T>()
                }
            );
        }
        self.disabled.remove(&TypeId::of::<T>());
        self
    }

    /// Removes the [`Plugin`] of type `T` (and any disabled state for it) from this builder.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    pub fn remove<T: Plugin>(mut self) -> Self {
        let id = TypeId::of::<T>();
        self.entries.retain(|e| e.id != id);
        self.disabled.remove(&id);
        self
    }

    /// Builds `group` and appends all of its [`Plugin`] entries and disabled flags into this
    /// builder.
    ///
    /// # Panics
    /// Panics if any added plugin returns `true` from [`Plugin::is_unique`] and is already
    /// present in this builder.
    ///
    /// [`Plugin`]: crate::app::plugin::Plugin
    /// [`Plugin::is_unique`]: crate::app::plugin::Plugin::is_unique
    pub fn add_group(mut self, group: impl PluginGroup) -> Self {
        let sub_builder = group.build();

        for entry in sub_builder.entries {
            if sub_builder.disabled.contains(&entry.id) {
                self.disabled.insert(entry.id);
            }

            if entry.plugin.is_unique() && self.entries.iter().any(|e| e.id == entry.id) {
                panic!(
                    "{}",
                    PluginError::DuplicateUniquePlugin {
                        name: entry.plugin.name()
                    }
                );
            }

            self.entries.retain(|e| e.id != entry.id);
            self.entries.push(entry);
        }

        self
    }

    /// Consumes this builder and calls [`Plugin::build`] on each enabled plugin in order.
    ///
    /// # Panics
    /// Panics if any enabled unique plugin has already been registered on `app`.
    ///
    /// [`Plugin::build`]: crate::app::plugin::Plugin::build
    pub fn finish(self, app: &mut App) {
        for entry in self.entries {
            if !self.disabled.contains(&entry.id) {
                if entry.plugin.is_unique() {
                    if app.registered_plugins.insert(entry.id) {
                        entry.plugin.build(app);
                    } else {
                        panic!(
                            "{}",
                            PluginError::PluginAlreadyInitialized {
                                name: entry.plugin.name()
                            }
                        );
                    }
                } else {
                    entry.plugin.build(app);
                }
            }
        }
    }
}

macro_rules! impl_plugin_group_tuple {
    ($($T:ident),*) => {
        impl<$($T: PluginGroup),*> PluginGroup for ($($T,)*) {
            fn build(self) -> PluginGroupBuilder {
                #[allow(non_snake_case)]
                let ($($T,)*) = self;
                let mut combined = PluginGroupBuilder::new_internal();
                $(
                    let sub_builder = $T.build();
                    combined = combined.add_group(sub_builder);
                )*
                combined
            }
        }
    };
}

impl_plugin_group_tuple!(A);
impl_plugin_group_tuple!(A, B);
impl_plugin_group_tuple!(A, B, C);
impl_plugin_group_tuple!(A, B, C, D);
impl_plugin_group_tuple!(A, B, C, D, E);
impl_plugin_group_tuple!(A, B, C, D, E, F);
impl_plugin_group_tuple!(A, B, C, D, E, F, G);
impl_plugin_group_tuple!(A, B, C, D, E, F, G, H);
