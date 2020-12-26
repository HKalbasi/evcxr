use ra_ap_syntax::{ast, SmolStr};

// Copyright 2020 The Evcxr Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Import {
    /// use x as _;
    /// use x::*;
    Unnamed(String),
    /// use x::y;
    /// use x::y as z;
    Named { name: String, code: String },
}

impl Import {
    fn format(name: &SmolStr, path: &[SmolStr]) -> Import {
        let code;
        let joined_path = path.join("::");
        if path.last() == Some(name) {
            code = format!("use {};", joined_path);
        } else {
            code = format!("use {} as {};", joined_path, name);
        }
        if name == "_" || name == "*" {
            Import::Unnamed(code)
        } else {
            Import::Named {
                name: name.to_string(),
                code,
            }
        }
    }
}

pub(crate) fn use_tree_names_do(use_tree: &ast::UseTree, out: &mut impl FnMut(Import)) {
    fn process_use_tree(use_tree: &ast::UseTree, prefix: &[SmolStr], out: &mut impl FnMut(Import)) {
        if let Some(path) = use_tree.path() {
            // If we get ::self, ignore it and use what we've got so far.
            if path.segment().and_then(|segment| segment.kind())
                == Some(ast::PathSegmentKind::SelfKw)
            {
                if let Some(last) = prefix.last() {
                    out(Import::format(last, prefix));
                }
                return;
            }

            // Collect the components of `path`.
            let mut path = path;
            let mut path_parts = Vec::new();
            loop {
                if let Some(segment) = path.segment() {
                    if let Some(name_ref) = segment.name_ref() {
                        path_parts.push(name_ref.text().clone());
                    }
                    if let Some(qualifier) = path.qualifier() {
                        path = qualifier;
                        continue;
                    }
                }
                break;
            }
            path_parts.reverse();

            // Combine the existing prefix with the new path components.
            let mut new_prefix = Vec::with_capacity(prefix.len() + path_parts.len());
            new_prefix.extend(prefix.iter().cloned());
            new_prefix.extend(path_parts.drain(..));

            // Recurse into any subtree.
            if let Some(tree_list) = use_tree.use_tree_list() {
                for subtree in tree_list.use_trees() {
                    process_use_tree(&subtree, &new_prefix, out);
                }
            } else if let Some(rename) = use_tree.rename() {
                if let Some(name) = ast::NameOwner::name(&rename) {
                    out(Import::format(name.text(), &new_prefix));
                } else if let Some(underscore) = rename.underscore_token() {
                    out(Import::format(underscore.text(), &new_prefix));
                }
            } else if let Some(star_token) = use_tree.star_token() {
                new_prefix.push(star_token.text().clone());
                out(Import::format(star_token.text(), &new_prefix));
            } else {
                out(Import::format(new_prefix.last().unwrap(), &new_prefix));
            }
        }
    }

    process_use_tree(use_tree, &[], out);
}

#[cfg(test)]
mod tests {
    use super::{use_tree_names_do, Import};
    use ra_ap_syntax::ast;

    fn use_tree_names(code: &str) -> Vec<Import> {
        let mut out = Vec::new();
        let item = ast::Item::parse(code).unwrap();
        if let ast::Item::Use(use_stmt) = item {
            if let Some(use_tree) = use_stmt.use_tree() {
                use_tree_names_do(&use_tree, &mut |import| {
                    out.push(import);
                });
            }
        }
        out
    }

    fn unnamed(code: &str) -> Import {
        Import::Unnamed(code.to_owned())
    }

    fn named(name: &str, code: &str) -> Import {
        Import::Named {
            name: name.to_owned(),
            code: code.to_owned(),
        }
    }

    #[test]
    fn test_complex_tree() {
        assert_eq!(
            use_tree_names(
                "use std::collections::{self, hash_map::{HashMap}, HashSet as MyHashSet};"
            ),
            vec![
                named("collections", "use std::collections;"),
                named("HashMap", "use std::collections::hash_map::HashMap;"),
                named("MyHashSet", "use std::collections::HashSet as MyHashSet;")
            ]
        );
    }

    #[test]
    fn test_underscore() {
        assert_eq!(
            use_tree_names("use foo::bar::MyTrait as _;"),
            vec![unnamed("use foo::bar::MyTrait as _;"),]
        );
    }

    #[test]
    fn test_glob() {
        assert_eq!(
            use_tree_names("use foo::bar::*;"),
            vec![unnamed("use foo::bar::*;"),]
        );
    }
}
