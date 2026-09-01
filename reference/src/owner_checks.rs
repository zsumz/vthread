//! Downstream names, traits, borrowed children, and owner admission compile checks.

use std::{
    rc::Rc,
    time::{Duration, Instant},
};
use vthread::{CancellationToken, Error, Result, Runtime, ScopeOptions, SpawnOptions, Spawner};

fn transferable<T: Send + Sync + Clone>() {}

pub(crate) fn verify() -> Result<()> {
    transferable::<Spawner>();
    transferable::<CancellationToken>();
    let runtime = Runtime::builder()
        .max_vthreads(8)
        .max_owned_scopes(2)
        .stack_cache_capacity(8)
        .build()?;
    assert_eq!(runtime.config().max_owned_scopes(), 2);
    let options = SpawnOptions::default().deadline(Instant::now() + Duration::from_secs(5));
    let capability = runtime.run_scope(|scope| {
        let capability: Spawner = scope.spawner();
        let mut child = scope.spawn_with(options, "borrowed owner", move || {
            let value = Rc::new(42);
            vthread::local_scope(|local| {
                let mut borrowed =
                    local.spawn_with(options, "borrowed result", || Rc::clone(&value))?;
                let token: CancellationToken = borrowed.cancellation_token();
                assert_eq!(*borrowed.join()?, 42);
                borrowed.cancel();
                assert!(token.is_cancelled());
                borrowed.wait()
            })
        })?;
        child.join()??;
        child.cancel();
        assert!(child.cancellation_token().is_cancelled());
        child.wait()?;
        Ok(capability)
    })?;
    assert!(matches!(
        capability.spawn("closed", || ()),
        Err(Error::ScopeClosed)
    ));
    let default_owner = runtime.supervisor()?;
    let configured = runtime.supervisor_with(ScopeOptions::default())?;
    let mut first = default_owner
        .spawner()
        .spawn_with(options, "default owner", || 42)?;
    let mut second = configured.spawn_with(options, "configured owner", || 43)?;
    assert_eq!(first.join()?, 42);
    assert_eq!(second.join()?, 43);
    default_owner.shutdown()?;
    configured.shutdown()?;
    runtime.shutdown()?;
    Ok(())
}

#[cfg(test)]
#[path = "owner_checks_test.rs"]
mod owner_checks_test;
