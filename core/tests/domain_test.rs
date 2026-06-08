fn v(d: &str) {
    assert!(sidedns_core::validate_domain(d), "'{d}' should be valid");
}
fn i(d: &str) {
    assert!(!sidedns_core::validate_domain(d), "'{d}' should be invalid");
}

#[test]
fn localhost() {
    v("localhost");
}
#[test]
fn single_label() {
    i("api");
}
#[test]
fn two_labels() {
    v("api.local");
}
#[test]
fn three_labels() {
    v("a.b.local");
}
#[test]
fn digits() {
    v("api2.local");
}
#[test]
fn hyphen() {
    v("my-api.local");
}
#[test]
fn uppercase() {
    v("API.LOCAL");
}
#[test]
fn mixed_case() {
    v("Api.Local");
}
#[test]
fn wildcard_one() {
    i("*.local");
}
#[test]
fn wildcard_two() {
    v("*.api.local");
}
#[test]
fn wildcard_tld() {
    v("*.example.com");
}

#[test]
fn empty() {
    i("");
}
#[test]
fn only_dot() {
    i(".");
}
#[test]
fn leading_dot() {
    i(".api.local");
}
#[test]
fn trailing_dot() {
    i("api.local.");
}
#[test]
fn double_dot() {
    i("api..local");
}
#[test]
fn leading_hyphen() {
    i("-api.local");
}
#[test]
fn trailing_hyphen() {
    i("api-.local");
}
#[test]
fn space() {
    i("api local");
}
#[test]
fn at_sign() {
    i("user@host.local");
}
#[test]
fn slash() {
    i("api/local");
}
#[test]
fn backslash() {
    i("api\\local");
}
#[test]
fn colon() {
    i("api:local");
}
#[test]
fn underscore() {
    i("my_api.local");
}
#[test]
fn bare_star() {
    i("*");
}
#[test]
fn star_no_dot() {
    i("*local");
}
#[test]
fn double_star() {
    i("**.local");
}
#[test]
fn star_middle() {
    i("api.*.local");
}
#[test]
fn star_dot_star() {
    i("*.*.local");
}
#[test]
fn exclamation() {
    i("api!.local");
}
