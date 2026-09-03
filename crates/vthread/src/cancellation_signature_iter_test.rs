use super::super::Signature;

#[test]
fn iteration_covers_inline_and_tree_signatures() {
    assert_eq!(Signature::singleton(7).iter().collect::<Vec<_>>(), [7]);
    let mut ids = Signature::singleton(7)
        .union(&Signature::singleton(9))
        .iter()
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids, [7, 9]);
}
