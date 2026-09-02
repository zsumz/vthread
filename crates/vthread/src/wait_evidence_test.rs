use super::SelectionRejection;

#[test]
fn selection_rejections_remain_distinct() {
    assert_ne!(SelectionRejection::NoWait, SelectionRejection::Retired);
    assert_ne!(SelectionRejection::NoActive, SelectionRejection::Selected);
}
