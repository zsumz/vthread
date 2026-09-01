use super::Counter;

#[test]
fn counter_reports_each_expensive_operation() {
    let mut counter = Counter::new();
    counter.union_item();
    counter.equality_node();
    counter.allocated_node();
    let work = counter.finish();
    assert_eq!(work.union_items, 1);
    assert_eq!(work.equality_nodes, 1);
    assert_eq!(work.allocated_nodes, 1);
}
