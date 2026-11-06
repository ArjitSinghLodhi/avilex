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

pub trait Plugin: 'static {
    fn build(&self, app: &mut App);

    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

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

pub trait PluginGroup: Sized {
    fn build(self) -> PluginGroupBuilder;

    fn name() -> String {
        type_name::<Self>().to_string()
    }

    fn set<T: Plugin>(self, plugin: T) -> PluginGroupBuilder {
        self.build().set(plugin)
    }

    fn disable<T: Plugin>(self) -> PluginGroupBuilder {
        self.build().disable::<T>()
    }

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

    pub fn start<PG: PluginGroup>() -> Self {
        Self::new_internal()
    }

    pub fn contains<T: Plugin>(&self) -> bool {
        let target_id = TypeId::of::<T>();
        self.entries.iter().any(|e| e.id == target_id)
    }

    pub fn enabled<T: Plugin>(&self) -> bool {
        let target_id = TypeId::of::<T>();
        self.contains::<T>() && !self.disabled.contains(&target_id)
    }

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

    pub fn try_add<T: Plugin>(self, plugin: T) -> Result<Self, (Self, T)> {
        if self.contains::<T>() {
            return Err((self, plugin));
        }
        Ok(self.add(plugin))
    }

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

    pub fn try_set<T: Plugin>(mut self, plugin: T) -> Result<Self, (Self, T)> {
        let id = TypeId::of::<T>();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.plugin = Box::new(plugin);
            Ok(self)
        } else {
            Err((self, plugin))
        }
    }

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

    pub fn try_add_before<Target: Plugin, Insert: Plugin>(
        self,
        plugin: Insert,
    ) -> Result<Self, (Self, Insert)> {
        if self.contains::<Insert>() || !self.contains::<Target>() {
            return Err((self, plugin));
        }
        Ok(self.add_before::<Target, Insert>(plugin))
    }

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

    pub fn try_add_after<Target: Plugin, Insert: Plugin>(
        self,
        plugin: Insert,
    ) -> Result<Self, (Self, Insert)> {
        if self.contains::<Insert>() || !self.contains::<Target>() {
            return Err((self, plugin));
        }
        Ok(self.add_after::<Target, Insert>(plugin))
    }

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

    pub fn remove<T: Plugin>(mut self) -> Self {
        let id = TypeId::of::<T>();
        self.entries.retain(|e| e.id != id);
        self.disabled.remove(&id);
        self
    }

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
                    combined.disabled.extend(sub_builder.disabled);
                    for entry in sub_builder.entries {
                        if entry.plugin.is_unique() && combined.entries.iter().any(|e| e.id == entry.id) {
                            panic!("{}", PluginError::DuplicateUniquePlugin { name: entry.plugin.name() });
                        }
                        combined.entries.retain(|e| e.id != entry.id);
                        combined.entries.push(entry);
                    }
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
