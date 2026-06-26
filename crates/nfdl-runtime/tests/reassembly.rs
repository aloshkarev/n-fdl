use nfdl_runtime::Reassembler;

#[test]
fn reassembly_basic_ooo() {
    let mut r = Reassembler::new(1000);
    r.accept_segment(1002, vec![2, 3]).unwrap();
    r.accept_segment(1000, vec![0, 1]).unwrap();
    assert_eq!(r.get_contiguous(), &[0, 1, 2, 3]);
}

#[test]
fn reassembly_need_more() {
    let mut r = Reassembler::new(0);
    r.accept_segment(0, vec![1, 2]).unwrap();
    assert_eq!(r.bytes_needed(5), Some(3));
}

#[test]
fn reassembly_compact() {
    let mut r = Reassembler::new(0);
    r.accept_segment(0, vec![0; 100]).unwrap();
    r.compact(20);
    assert_eq!(r.get_contiguous().len(), 20);
}
