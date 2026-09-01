#[test]
fn application_errors_remain_borrowed_caller_owned_values() {
    super::generic_body_errors().unwrap();
}

#[test]
fn top_level_application_errors_preserve_all_owned_parts() {
    super::generic_application_errors();
}

#[test]
fn io_ownership_and_inert_reports_are_accessible_downstream() {
    super::caller_owned_io_sources().unwrap();
}

#[test]
fn default_primitive_waiter_budgets_are_visible_downstream() {
    super::default_waiter_budgets().unwrap();
}
