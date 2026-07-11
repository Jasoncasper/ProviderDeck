use providerdeck_core::local_auth::{authorization_matches, runtime_bearer_token};

#[test]
fn authorization_accepts_only_the_exact_runtime_bearer() {
    let request = "POST /provider/team/v1/responses HTTP/1.1\r\nAuthorization: Bearer runtime-secret\r\n\r\n{}";

    assert!(authorization_matches(request, "runtime-secret"));
    assert!(!authorization_matches(request, "different"));
    assert!(!authorization_matches(
        "POST / HTTP/1.1\r\n\r\n{}",
        "runtime-secret"
    ));
}

#[test]
fn generated_runtime_bearer_is_stable_for_the_process_and_not_empty() {
    let first = runtime_bearer_token();
    let second = runtime_bearer_token();

    assert_eq!(first, second);
    assert!(first.len() >= 32);
}
