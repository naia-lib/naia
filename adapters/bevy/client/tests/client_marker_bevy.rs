/// Compile test: derive ClientMarkerBevy on a dummy marker and verify that the
/// generated type aliases resolve against `naia_bevy_client::` paths.
/// This is the proof that a non-Diax naia-bevy project can use the derive.
#[derive(Clone, Copy, PartialEq, Eq, naia_bevy_client::ClientMarker)]
struct TestMarker;

// Reference a selection of the generated aliases to force the compiler to
// resolve each one against naia_bevy_client paths.
fn _assert_aliases(
    _c: TestMarkerClient,
    _conn: TestMarkerConnectEvent,
    _disc: TestMarkerDisconnectEvent,
    _rej: TestMarkerRejectEvent,
    _spawn: TestMarkerSpawnEntityEvent,
    _despawn: TestMarkerDespawnEntityEvent,
    _err: TestMarkerErrorEvent,
    _msg: TestMarkerMessageEvents,
    _req: TestMarkerRequestEvents,
    _ctick: TestMarkerClientTickEvent,
    _stick: TestMarkerServerTickEvent,
) {
}

// Verify the AppBundleExt trait name resolves and is implementable on App.
fn _assert_app_ext(app: &mut bevy_app::App) -> &mut bevy_app::App
where
    bevy_app::App: TestMarkerAppBundleExt,
{
    app
}
