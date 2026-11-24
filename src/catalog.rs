// SPDX-License-Identifier: EUPL-1.2
//! What is known of relations' names and columns as each write is read.

use std::collections::HashMap;

use polyglot_sql::validation::ValidationSchema;
use serde::Deserialize;
use serde_json::json;

use crate::answer::Answer;
use crate::dialect::Dialect;
use crate::relation::{Name, RelationName, repeated};
use crate::write::Write;

/// The request's form of a relation of its catalog: its name and its columns.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declared {
    relation: String,
    columns: Vec<String>,
}

/// The relations known, each with its columns, under the name that made it known.
#[derive(Clone, Default)]
pub struct Catalog {
    relations: HashMap<RelationName, Vec<Name>>,
}

impl Catalog {
    /// The catalog that the request declares, or why the request is refused.
    pub fn declared(declared: &[Declared], dialect: Dialect, defaults: &[Name]) -> Result<Catalog, String> {
        let mut catalog = Catalog { relations: HashMap::new() };
        for entry in declared {
            let relation = RelationName::read(&entry.relation, dialect, defaults)?;
            let columns =
                (entry.columns.iter()).map(|column| Name::read(column, dialect)).collect::<Result<Vec<_>, _>>()?;
            if catalog.declare(&relation, columns)? {
                return Err(format!("the catalog declares {relation} twice"));
            }
        }
        Ok(catalog)
    }

    /// Declares `relation` with `columns`, in place of an earlier declaration, and says
    /// whether there was one; or why it cannot: no column, or a column twice.
    pub fn declare(&mut self, relation: &RelationName, columns: Vec<Name>) -> Result<bool, String> {
        if columns.is_empty() {
            return Err(format!("the relation {relation} has no column"));
        }
        if let Some(column) = repeated(&columns) {
            return Err(format!("the relation {relation} declares {column} twice"));
        }
        let earlier = self.relations.remove(relation).is_some();
        self.relations.insert(relation.clone(), columns);
        Ok(earlier)
    }

    /// The relation as the catalog knows it, and its columns, if it knows it.
    pub fn get(&self, relation: &RelationName) -> Option<(&RelationName, &[Name])> {
        self.relations.get_key_value(relation).map(|(known, columns)| (known, columns.as_slice()))
    }

    /// What the engine is told of the known relations among `relations`: each under its
    /// parts, quoted, and under its full name, which a `*` in a CTE needs, with its
    /// columns, each quoted as its parts are.
    pub fn schema(&self, relations: &[RelationName]) -> ValidationSchema {
        let tables: Vec<_> = (relations.iter().filter_map(|relation| self.get(relation)))
            .map(|(known, columns)| {
                let quoted = known.quoted();
                let mut parts = quoted.iter().rev();
                let columns: Vec<_> = columns.iter().map(|column| json!({"name": column.quoted()})).collect();
                json!({"name": parts.next(), "schema": parts.next(), "aliases": [quoted.join(".")], "columns": columns})
            })
            .collect();
        serde_json::from_value(json!({"tables": tables, "strict": true})).expect("a schema of names and columns")
    }

    /// Records what a write makes of its target: one that `replaces` it, its output
    /// columns, or unknown; any other, nothing.
    pub fn record(&mut self, target: RelationName, replaces: bool, columns: Option<Vec<Name>>) {
        if !replaces {
            return;
        }
        self.relations.remove(&target);
        if let Some(columns) = columns {
            self.relations.insert(target, columns);
        }
    }

    /// The answer to `write`, whose target is `target`, against the catalog as it stands
    /// just before it, once what the write makes of its target is recorded.
    #[tracing::instrument(level = "debug", skip_all, fields(id = %write.id))]
    pub fn answer(&mut self, write: &Write, target: RelationName, dialect: Dialect, defaults: &[Name]) -> Answer {
        let (answer, replaces) = Answer::of(write, &target, self, dialect, defaults);
        let (edges, gaps, refused) = (answer.edges.len(), answer.gaps.len(), answer.refusal.is_some());
        // A refused write's mode is not known; the catalog records it by its own rule, as a
        // replace one only when it does not parse.
        tracing::debug!(replaces = (!refused).then_some(replaces), edges, gaps, refused, "answered");
        self.record(target, replaces, answer.columns.clone());
        answer
    }
}
