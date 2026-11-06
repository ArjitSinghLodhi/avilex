use avilex::prelude::*;

#[derive(Resource, Default)]
struct TestCounter(u32);

#[derive(Debug)]
struct PluginA;
impl Plugin for PluginA {
    fn build(&self, app: &mut App) {
        if let Some(mut counter) = app.world().get_resource_mut::<TestCounter>() {
            counter.0 += 1;
        }
    }
}

#[derive(Debug)]
struct PluginB;
impl Plugin for PluginB {
    fn build(&self, app: &mut App) {
        if let Some(mut counter) = app.world().get_resource_mut::<TestCounter>() {
            counter.0 += 10;
        }
    }
}

#[derive(Debug)]
struct PluginC;
impl Plugin for PluginC {
    fn build(&self, app: &mut App) {
        if let Some(mut counter) = app.world().get_resource_mut::<TestCounter>() {
            counter.0 += 100;
        }
    }
}

struct NonUniquePlugin;
impl Plugin for NonUniquePlugin {
    fn build(&self, app: &mut App) {
        if let Some(mut counter) = app.world().get_resource_mut::<TestCounter>() {
            counter.0 += 1000;
        }
    }
    fn is_unique(&self) -> bool {
        false
    }
}

struct GroupChild;
impl PluginGroup for GroupChild {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>().add(PluginA)
    }
}

struct GroupParent;
impl PluginGroup for GroupParent {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add_group(GroupChild)
            .add(PluginB)
    }
}

struct GroupA;
impl PluginGroup for GroupA {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(PluginA)
            .add(PluginB)
    }
}

struct GroupB;
impl PluginGroup for GroupB {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>().add(PluginC)
    }
}

#[test_fork::test]
fn test_single_plugin() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins(PluginA);
    assert_eq!(app.world().resource::<TestCounter>().0, 1);
}

#[test_fork::test]
fn test_tuples() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins((PluginA, PluginB, PluginC));
    assert_eq!(app.world().resource::<TestCounter>().0, 111);
}

#[test_fork::test]
fn test_plugin_groups() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins(GroupA);
    assert_eq!(app.world().resource::<TestCounter>().0, 11);
}

#[test_fork::test]
fn test_nested_plugin_groups() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins(GroupParent);
    assert_eq!(app.world().resource::<TestCounter>().0, 11);
}

#[test_fork::test]
fn test_registering_group_inside_another_group() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let combined_group = PluginGroupBuilder::start::<GroupA>()
        .add_group(GroupA)
        .add_group(GroupB);
    app.add_plugins(combined_group);
    assert_eq!(app.world().resource::<TestCounter>().0, 111);
}

#[test_fork::test]
fn test_mixed_of_groups_and_plugins_in_tuple() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins((GroupA.disable::<PluginA>(), GroupB));
    assert_eq!(app.world().resource::<TestCounter>().0, 110);
}

#[test_fork::test]
fn test_disabling_plugins() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins(GroupA.disable::<PluginB>());
    assert_eq!(app.world().resource::<TestCounter>().0, 1);
}

#[test_fork::test]
fn test_disable_cascades_through_nested_groups() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins(GroupParent.disable::<PluginA>());
    assert_eq!(app.world().resource::<TestCounter>().0, 10);
}

#[test_fork::test]
fn test_enable_after_disable() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins(GroupA.disable::<PluginB>().enable::<PluginB>());
    assert_eq!(app.world().resource::<TestCounter>().0, 11);
}

#[test_fork::test]
fn test_remove_plugin() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    app.add_plugins(GroupA.build().remove::<PluginB>());
    assert_eq!(app.world().resource::<TestCounter>().0, 1);
}

#[test_fork::test]
fn test_chained_explicit_ordering_before_and_after() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let builder = PluginGroupBuilder::start::<GroupA>()
        .add(PluginB)
        .add_before::<PluginB, PluginA>(PluginA)
        .add_after::<PluginB, PluginC>(PluginC);
    app.add_plugins(builder);
    assert_eq!(app.world().resource::<TestCounter>().0, 111);
}

#[test_fork::test]
fn test_explicit_ordering_before() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let builder = PluginGroupBuilder::start::<GroupA>()
        .add(PluginB)
        .add_before::<PluginB, PluginA>(PluginA);
    app.add_plugins(builder);
    assert_eq!(app.world().resource::<TestCounter>().0, 11);
}

#[test_fork::test]
fn test_explicit_ordering_after() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let builder = PluginGroupBuilder::start::<GroupA>()
        .add(PluginA)
        .add_after::<PluginA, PluginB>(PluginB);
    app.add_plugins(builder);
    assert_eq!(app.world().resource::<TestCounter>().0, 11);
}

#[test_fork::test]
fn test_overwrite_with_set() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let builder = PluginGroupBuilder::start::<GroupA>()
        .add(PluginA)
        .set(PluginA);
    app.add_plugins(builder);
    assert_eq!(app.world().resource::<TestCounter>().0, 1);
}

#[test_fork::test]
fn test_try_add_before_overwrite() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let builder = GroupA
        .build()
        .try_add_before_overwrite::<PluginB, PluginA>(PluginA)
        .unwrap();
    app.add_plugins(builder);
    assert_eq!(app.world().resource::<TestCounter>().0, 11);
}

#[test_fork::test]
fn test_try_add_after_overwrite() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let builder = GroupA
        .build()
        .try_add_after_overwrite::<PluginA, PluginB>(PluginB)
        .unwrap();
    app.add_plugins(builder);
    assert_eq!(app.world().resource::<TestCounter>().0, 11);
}

#[test_fork::test]
fn test_non_unique_multiple_instances() {
    let mut app = App::new();
    app.world().insert_resource(TestCounter(0));
    let builder = PluginGroupBuilder::start::<GroupA>().add(NonUniquePlugin);
    let builder = builder
        .try_add(NonUniquePlugin)
        .unwrap_or_else(|(b, p)| b.add(p));
    app.add_plugins(builder);
    assert_eq!(app.world().resource::<TestCounter>().0, 2000);
}

#[test_fork::test]
#[should_panic(expected = "DuplicateUniquePlugin")]
fn panic_on_duplicate_unique_in_builder() {
    let _ = PluginGroupBuilder::start::<GroupA>()
        .add(PluginA)
        .add(PluginA);
}

#[test_fork::test]
#[should_panic(expected = "DuplicateUniquePlugin")]
fn panic_on_duplicate_unique_in_tuple() {
    let mut app = App::new();
    app.add_plugins((PluginA, PluginA));
}

#[test_fork::test]
#[should_panic(expected = "PluginAlreadyInitialized")]
fn panic_on_double_registration_across_calls() {
    let mut app = App::new();
    app.add_plugins(PluginA);
    app.add_plugins(PluginA);
}

#[test_fork::test]
#[should_panic(expected = "PluginDoesNotExist")]
fn panic_on_disabling_missing_plugin() {
    let _ = GroupA.disable::<NonUniquePlugin>();
}

#[test_fork::test]
#[should_panic(expected = "TargetPluginNotFound")]
fn panic_on_invalid_ordering_target() {
    let _ = PluginGroupBuilder::start::<GroupA>().add_before::<PluginB, PluginA>(PluginA);
}
