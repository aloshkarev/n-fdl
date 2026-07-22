use nfdl_runtime::{FlowKey, SessionDb};

#[test]
fn basic_session_db() {
    let mut db = SessionDb::new(1000);
    let key = FlowKey { data: [0; 16] };

    let (sess, evicted) = db.get_or_create(key.clone());
    assert!(evicted.is_none());
    sess.state = 1;

    db.transition(&key, 2, Some(("req_auth".into(), 0xDEAD_BEEF)));

    assert_eq!(db.len(), 1);
    let (sess2, _) = db.get_or_create(key);
    assert_eq!(sess2.state, 2);
    assert_eq!(sess2.vars.get("req_auth"), Some(&0xDEAD_BEEF));
}
