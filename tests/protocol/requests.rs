// SPDX-License-Identifier: EUPL-1.2
//! A request read from standard input, or refused as a whole, as `docs/protocol.md`
//! says; each refusal a valid request with one defect.

use serde_json::{Value, json};

use super::{answers, assert_refused, outline, read, request, respond};

/// A valid request: the catalog `s` of the column `a`, and the write `t`.
fn valid() -> Value {
    request(&[("s", &["a"])], &[("t", "select a from s")])
}

/// Whether the binary refuses `request` as a whole, naming `naming`.
fn refused(request: &Value, naming: &str) {
    assert_refused(&answers(request.to_string().as_bytes()), naming);
}

#[test]
fn answers_a_request_on_standard_input() {
    let output = answers(
        request(&[("s", &["a"])], &[("t", "select a from s"), ("u", "select a from t")]).to_string().as_bytes(),
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    let response: Value = serde_json::from_slice(&output.stdout).expect("the response is JSON");
    assert_eq!(response["protocol"], 1);
    let fields = |answer: &Value| answer.as_object().map(|answer| answer.keys().cloned().collect::<Vec<_>>());
    let answers = response["answers"].as_array().expect("answers");
    assert_eq!(answers.iter().map(|answer| answer["write"].clone()).collect::<Vec<_>>(), [json!("t"), json!("u")]);
    assert!(answers.iter().all(|answer| fields(answer).is_some_and(|fields| fields.len() == 5)), "{answers:?}");
}

#[test]
fn refuses_malformed_json() {
    assert_refused(&answers(b"{\"protocol\": 1, \"dialect\": "), "JSON");
}

#[test]
fn refuses_input_that_is_not_utf8() {
    let text = request(&[("s", &["a"])], &[("t", "select a from s -- #")]).to_string();
    assert_refused(
        &answers(&text.bytes().map(|byte| if byte == b'#' { 0xff } else { byte }).collect::<Vec<_>>()),
        "UTF-8",
    );
}

#[test]
fn refuses_another_protocol_before_an_unknown_field() {
    let mut request = valid();
    (request["protocol"], request["mode"]) = (json!(2), json!("replace"));
    refused(&request, "protocol 2");
}

#[test]
fn refuses_a_missing_protocol() {
    let mut request = valid();
    request.as_object_mut().expect("an object").remove("protocol");
    refused(&request, "protocol");
}

#[test]
fn refuses_a_protocol_that_is_not_an_integer() {
    let mut request = valid();
    request["protocol"] = json!(1.5);
    refused(&request, "1.5");
}

#[test]
fn refuses_an_unknown_field() {
    let mut request = valid();
    request["writes"][0]["mode"] = json!("replace");
    refused(&request, "mode");
}

#[test]
fn refuses_a_missing_field() {
    let mut request = valid();
    request.as_object_mut().expect("an object").remove("writes");
    refused(&request, "writes");
}

#[test]
fn refuses_a_mistyped_field() {
    let mut request = valid();
    request["catalog"][0]["columns"] = json!("a");
    refused(&request, "invalid type");
}

#[test]
fn refuses_the_generic_dialect() {
    let mut request = valid();
    request["dialect"] = json!("generic");
    refused(&request, "generic");
}

#[test]
fn refuses_a_dialect_in_capitals() {
    let mut request = valid();
    request["dialect"] = json!("SNOWFLAKE");
    refused(&request, "SNOWFLAKE");
}

#[test]
fn refuses_a_default_that_is_not_one_identifier() {
    let mut request = valid();
    request["defaults"] = json!({"database": "db", "schema": "s.t"});
    refused(&request, "s.t");
}

#[test]
fn refuses_a_database_default_without_a_schema() {
    let mut request = valid();
    request["defaults"] = json!({"database": "db"});
    refused(&request, "schema");
}

#[test]
fn refuses_a_relation_name_that_is_not_bare() {
    refused(&request(&[("s join u on 1 = 1", &["a"])], &[("t", "select a from s")]), "s join u on 1 = 1");
}

#[test]
fn refuses_a_name_with_text_around_it() {
    for name in [" s", "s ", "s /* a note */", "db . s"] {
        refused(&request(&[(name, &["a"])], &[("t", "select a from s")]), &format!("{name:?}"));
    }
}

#[test]
fn refuses_a_target_that_is_not_bare() {
    refused(&request(&[("s", &["a"])], &[("t as x", "select a from s")]), "t as x");
}

#[test]
fn refuses_a_column_name_that_is_not_one_identifier() {
    refused(&request(&[("s", &["s.a"])], &[("t", "select a from s")]), "s.a");
}

#[test]
fn refuses_a_relation_declared_twice() {
    refused(&request(&[("s", &["a"]), ("S", &["a"])], &[("t", "select a from s")]), "S twice");
}

#[test]
fn refuses_a_column_declared_twice() {
    refused(&request(&[("s", &["a", "A"])], &[("t", "select a from s")]), "A twice");
}

#[test]
fn refuses_a_relation_with_no_column() {
    refused(&request(&[("s", &[])], &[("t", "select a from s")]), "no column");
}

#[test]
fn refuses_two_writes_with_one_id() {
    let mut request = request(&[("s", &["a"])], &[("t", "select a from s"), ("u", "select a from s")]);
    request["writes"][1]["id"] = json!("t");
    refused(&request, "id \"t\"");
}

#[test]
fn two_writes_with_one_target_are_read_in_order() {
    let writes = [("t", "create table t as select a from s"), ("T", "insert into t select b from u")];
    let mut request = request(&[("s", &["a"]), ("u", &["b"])], &writes);
    request["writes"][1]["id"] = json!("t.insert");
    let answers = respond(&request);
    assert_eq!(
        answers.iter().map(|answer| answer["write"].clone()).collect::<Vec<_>>(),
        [json!("t"), json!("t.insert")]
    );
    // The insert maps onto the columns that the table, read first, gave its target.
    assert_eq!(answers.iter().map(outline).collect::<Vec<_>>(), [vec!["s.a -> t.a copy"], vec!["u.b -> T.a copy"]]);
}

#[test]
fn protocol_document_example_holds() {
    let document = read("docs/protocol.md");
    let example = |section: &str| {
        let text = document.split(section).nth(1).and_then(|rest| rest.split("```json").nth(1)?.split("```").next());
        serde_json::from_str::<Value>(text.expect("an example")).expect("the example is JSON")
    };
    let output = answers(example("## The request").to_string().as_bytes());
    assert_eq!(serde_json::from_slice::<Value>(&output.stdout).ok(), Some(example("## The response")));
}
