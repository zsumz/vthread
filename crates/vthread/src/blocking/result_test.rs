use super::Output;
use std::sync::{Arc, Weak};

#[test]
fn transferred_result_can_reenter_its_completion_cell_on_drop() {
    struct Reenter(Weak<Output<Reenter>>);
    impl Drop for Reenter {
        fn drop(&mut self) {
            assert!(self.0.upgrade().unwrap().take().is_err());
        }
    }
    let output = Arc::new(Output::new());
    output.store(Ok(Reenter(Arc::downgrade(&output))));
    drop(output.take().unwrap());
}
