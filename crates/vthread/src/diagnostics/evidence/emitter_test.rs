use super::Emitter;

#[test]
fn emitter_preserves_its_carrier_context() {
    let (recorder, _) = crate::diagnostics::evidence::bounded(1);
    let emitter = Emitter::new(
        crate::identity::RuntimeId::next(),
        crate::CarrierId(3),
        recorder,
    );
    core::assert_eq!(emitter.carrier(), crate::CarrierId(3));
}
