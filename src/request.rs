// SPDX-License-Identifier: EUPL-1.2
//! The request read from standard input, refused as a whole at its boundary, and the
//! response that answers each of its writes.

use std::io::Read;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::answer::Answer;
use crate::catalog::{Catalog, Declared};
use crate::dialect::Dialect;
use crate::relation::{Defaults, RelationName, repeated};
use crate::write::Write;

/// The number of the request's and the response's format.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Protocol(u32);

impl Protocol {
    /// The protocols that this build reads.
    pub const READ: [Protocol; 1] = [Protocol(1)];
}

/// One request, as it is given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    protocol: Protocol,
    dialect: String,
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    catalog: Vec<Declared>,
    writes: Vec<Write>,
}

/// One answer per write of the request, in its order.
#[derive(Serialize)]
pub struct Response {
    protocol: Protocol,
    answers: Vec<Answer>,
}

impl Request {
    /// The request that `input` holds, read up to `limit` bytes, its protocol before any
    /// other field; or why it is refused.
    pub fn read(input: impl Read, limit: u64) -> Result<Request, String> {
        let mut bytes = Vec::new();
        input.take(limit + 1).read_to_end(&mut bytes).map_err(|error| format!("the request does not read: {error}"))?;
        if bytes.len() as u64 > limit {
            return Err(format!("the request is longer than {limit} bytes"));
        }
        let text = String::from_utf8(bytes).map_err(|error| format!("the request is not UTF-8: {error}"))?;
        let value: Value = serde_json::from_str(&text).map_err(|error| format!("the request is not JSON: {error}"))?;
        let protocol = value.get("protocol").ok_or("the request has no protocol")?;
        let number = protocol.as_u64().ok_or_else(|| format!("the protocol {protocol} is not an integer"))?;
        if !Protocol::READ.iter().any(|read| u64::from(read.0) == number) {
            return Err(format!("protocol {number} is not one that this build reads: {:?}", Protocol::READ));
        }
        serde_json::from_value(value).map_err(|error| format!("the request does not fit protocol {number}: {error}"))
    }

    /// The response to this request, or why it is refused as a whole.
    pub fn respond(self) -> Result<Response, String> {
        let dialect = (Dialect::named(&self.dialect)).ok_or_else(|| {
            format!("the dialect {:?} is not one of this build's: {:?}", self.dialect, Dialect::built())
        })?;
        let defaults = self.defaults.read(dialect)?;
        let mut catalog = Catalog::declared(&self.catalog, dialect, &defaults)?;
        let targets: Vec<RelationName> = (self.writes.iter())
            .map(|write| RelationName::read(&write.target, dialect, &defaults))
            .collect::<Result<_, _>>()?;
        if let Some(id) = repeated(self.writes.iter().map(|write| &write.id)) {
            return Err(format!("two writes have the id {id:?}"));
        }
        if let Some(target) = repeated(&targets) {
            return Err(format!("two writes have the target {target}"));
        }
        let answers = (self.writes.iter().zip(targets))
            .map(|(write, target)| {
                let (answer, replaces) = Answer::of(write, &target, &catalog, dialect, &defaults);
                catalog.record(target, replaces, answer.columns.clone());
                answer
            })
            .collect();
        Ok(Response { protocol: self.protocol, answers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUEST: &str = r#"{"protocol": 1, "dialect": "snowflake", "writes": []}"#;

    #[test]
    fn a_request_over_the_limit_is_refused() {
        let refused = Request::read(REQUEST.as_bytes(), REQUEST.len() as u64 - 1).err();
        assert!(refused.is_some_and(|reason| reason.contains(&(REQUEST.len() - 1).to_string())));
    }

    #[test]
    fn a_request_at_the_limit_is_read() {
        assert!(Request::read(REQUEST.as_bytes(), REQUEST.len() as u64).is_ok());
    }
}
