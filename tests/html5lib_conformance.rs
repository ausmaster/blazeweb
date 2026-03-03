//! html5lib tree-construction conformance tests.
//!
//! Parses each test case from the html5lib-tests .dat files, runs our
//! TreeSink parser, dumps the resulting arena tree in html5lib format,
//! and compares against expected output.

use std::fs;
use std::path::PathBuf;

// We access blazeweb's internal modules via the crate.
// This is an integration test, so we use the public API re-exported from lib.
// However, since lib.rs is a cdylib for PyO3, integration tests can't link
// against it directly. Instead we build this as a unit test inside the crate.
// See: tests are compiled separately, so we need to reference the crate.

/// A single html5lib tree construction test case.
#[derive(Debug)]
struct TestCase {
    /// The input HTML to parse.
    data: String,
    /// Expected tree dump lines (the #document section).
    expected_tree: String,
    /// Whether this is a fragment parsing test.
    fragment_context: Option<String>,
    /// Script mode, if specified.
    script_mode: Option<ScriptMode>,
    /// Line number in the .dat file where this test starts.
    line_number: usize,
    /// Source file name.
    file_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ScriptMode {
    On,
    Off,
}

/// Parse a .dat file into individual test cases.
fn parse_dat_file(content: &str, file_name: &str) -> Vec<TestCase> {
    let mut tests = Vec::new();

    // Collect all lines with their line numbers.
    let lines: Vec<&str> = content.lines().collect();

    // State: current section name and its accumulated content lines.
    let mut current_header: Option<String> = None;
    let mut current_body = String::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut test_start_line = 1usize;

    let is_section_header = |line: &str| -> bool {
        // Section headers start with # but not "#  " (which is indented tree content)
        line.starts_with('#')
            && !line.starts_with("#  ")
            && (line == "#data"
                || line == "#errors"
                || line == "#new-errors"
                || line == "#document"
                || line == "#document-fragment"
                || line == "#script-on"
                || line == "#script-off")
    };

    let flush_sections = |sections: &mut Vec<(String, String)>,
                          tests: &mut Vec<TestCase>,
                          start_line: usize,
                          file_name: &str| {
        if sections.is_empty() {
            return;
        }
        let mut data = String::new();
        let mut document = String::new();
        let mut fragment = None;
        let mut script = None;

        for (key, value) in sections.drain(..) {
            match key.as_str() {
                "#data" => data = value,
                "#document" => document = value,
                "#document-fragment" => fragment = Some(value.trim().to_string()),
                "#script-on" => script = Some(ScriptMode::On),
                "#script-off" => script = Some(ScriptMode::Off),
                "#errors" | "#new-errors" => {}
                _ => {}
            }
        }

        if !document.is_empty() {
            tests.push(TestCase {
                data,
                expected_tree: document,
                fragment_context: fragment,
                script_mode: script,
                line_number: start_line,
                file_name: file_name.to_string(),
            });
        }
    };

    for (i, &line) in lines.iter().enumerate() {
        let line_num = i + 1;

        if is_section_header(line) {
            // Save the previous section's body
            if let Some(header) = current_header.take() {
                sections.push((header, current_body.clone()));
                current_body.clear();
            }

            // If this is #data and we already have sections, flush the previous test
            if line == "#data" && !sections.is_empty() {
                flush_sections(&mut sections, &mut tests, test_start_line, file_name);
                test_start_line = line_num;
            } else if sections.is_empty() && current_header.is_none() {
                test_start_line = line_num;
            }

            current_header = Some(line.to_string());
            current_body.clear();
        } else {
            // Content line — append to current section body
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }

    // Finalize last section and last test
    if let Some(header) = current_header.take() {
        sections.push((header, current_body));
    }
    flush_sections(&mut sections, &mut tests, test_start_line, file_name);

    tests
}

/// Dump an arena tree in html5lib format.
fn dump_tree(arena: &_blazeweb::dom::Arena, node_id: _blazeweb::dom::NodeId, indent: usize, output: &mut String) {
    use _blazeweb::dom::node::NodeData;

    let node = &arena.nodes[node_id];
    let prefix = format!("| {}", "  ".repeat(indent));

    match &node.data {
        NodeData::Document => {
            // Don't print the document node itself, just recurse
            for child in arena.children(node_id) {
                dump_tree(arena, child, indent, output);
            }
        }
        NodeData::Doctype {
            name,
            public_id,
            system_id,
        } => {
            if !public_id.is_empty() || !system_id.is_empty() {
                output.push_str(&format!(
                    "{prefix}<!DOCTYPE {name} \"{public_id}\" \"{system_id}\">\n"
                ));
            } else {
                output.push_str(&format!("{prefix}<!DOCTYPE {name}>\n"));
            }
        }
        NodeData::Element(data) => {
            let ns = &data.name.ns;
            let local = &*data.name.local;

            // Namespace prefix
            let tag = if *ns == markup5ever::ns!(svg) {
                format!("svg {local}")
            } else if *ns == markup5ever::ns!(mathml) {
                format!("math {local}")
            } else {
                local.to_string()
            };

            output.push_str(&format!("{prefix}<{tag}>\n"));

            // Attributes sorted lexicographically
            let mut attrs: Vec<_> = data.attrs.iter().collect();
            attrs.sort_by(|a, b| {
                let a_name = format_attr_name(&a.name);
                let b_name = format_attr_name(&b.name);
                a_name.cmp(&b_name)
            });
            for attr in &attrs {
                let attr_name = format_attr_name(&attr.name);
                let attr_prefix = format!("| {}", "  ".repeat(indent + 1));
                output.push_str(&format!("{attr_prefix}{attr_name}=\"{}\"\n", &*attr.value));
            }

            // Template contents
            if let Some(template_contents) = data.template_contents {
                let content_prefix = format!("| {}", "  ".repeat(indent + 1));
                output.push_str(&format!("{content_prefix}content\n"));
                for child in arena.children(template_contents) {
                    dump_tree(arena, child, indent + 2, output);
                }
            }

            // Children
            for child in arena.children(node_id) {
                dump_tree(arena, child, indent + 1, output);
            }
        }
        NodeData::Text(text) => {
            output.push_str(&format!("{prefix}\"{text}\"\n"));
        }
        NodeData::Comment(text) => {
            output.push_str(&format!("{prefix}<!-- {text} -->\n"));
        }
    }
}

fn format_attr_name(name: &markup5ever::QualName) -> String {
    let ns = &name.ns;
    let local = &*name.local;

    if *ns == markup5ever::ns!(xlink) {
        format!("xlink {local}")
    } else if *ns == markup5ever::ns!(xml) {
        format!("xml {local}")
    } else if *ns == markup5ever::ns!(xmlns) {
        format!("xmlns {local}")
    } else {
        local.to_string()
    }
}

/// Find all .dat files in the tree-construction directory.
fn find_test_files() -> Vec<PathBuf> {
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("html5lib-tests")
        .join("tree-construction");

    if !test_dir.exists() {
        panic!(
            "html5lib-tests not found at {:?}. Run: git submodule update --init",
            test_dir
        );
    }

    let mut files: Vec<PathBuf> = fs::read_dir(&test_dir)
        .expect("failed to read test dir")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "dat") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    files.sort();
    files
}

#[test]
fn html5lib_tree_construction_conformance() {
    let test_files = find_test_files();
    assert!(!test_files.is_empty(), "no .dat test files found");

    let mut total = 0;
    let mut passed = 0;
    let skipped = 0;
    let mut failures: Vec<String> = Vec::new();

    for file_path in &test_files {
        let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();
        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|e| panic!("failed to read {file_name}: {e}"));

        let test_cases = parse_dat_file(&content, &file_name);

        for tc in &test_cases {
            total += 1;

            // Normalize: trim trailing whitespace from each line and trailing newline
            let normalize = |s: &str| -> String {
                s.lines()
                    .map(|l| l.trim_end())
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim_end()
                    .to_string()
            };

            let expected = normalize(&tc.expected_tree);

            // Determine which scripting modes to test
            let modes: Vec<bool> = match tc.script_mode {
                Some(ScriptMode::On) => vec![true],
                Some(ScriptMode::Off) => vec![false],
                None => vec![true, false], // Run both
            };

            let mut matched = false;
            let mut last_actual = String::new();
            for scripting_enabled in &modes {
                let arena = if let Some(ref ctx) = tc.fragment_context {
                    _blazeweb::dom::treesink::parse_fragment(
                        &tc.data, ctx, *scripting_enabled,
                    )
                } else {
                    _blazeweb::dom::treesink::parse_with_options(
                        &tc.data, *scripting_enabled,
                    )
                };
                let mut actual = String::new();
                // For fragment tests, dump children of the document (the context element
                // is not part of the output)
                if tc.fragment_context.is_some() {
                    // Fragment parse produces: Document -> <html> -> children
                    // The expected output is the children of the <html> element.
                    // Find the <html> element (first child of document)
                    if let Some(html_node) = arena.children(arena.document).next() {
                        for child in arena.children(html_node) {
                            dump_tree(&arena, child, 0, &mut actual);
                        }
                    }
                } else {
                    dump_tree(&arena, arena.document, 0, &mut actual);
                }
                let actual_norm = normalize(&actual);

                if expected == actual_norm {
                    matched = true;
                    break;
                }
                last_actual = actual_norm;
            }

            if matched {
                passed += 1;
            } else {
                let msg = format!(
                    "FAIL: {}:{}\n  input: {:?}\n  expected:\n{}\n  actual:\n{}\n",
                    tc.file_name,
                    tc.line_number,
                    tc.data,
                    expected,
                    last_actual,
                );
                failures.push(msg);
            }
        }
    }

    // Print summary
    let failed = failures.len();
    eprintln!(
        "\nhtml5lib conformance: {passed}/{total} passed, {failed} failed, {skipped} skipped"
    );

    if !failures.is_empty() {
        // Print all failures
        let show = failures.len();
        for f in &failures[..show] {
            eprintln!("{f}");
        }
        if failures.len() > show {
            eprintln!("... and {} more failures", failures.len() - show);
        }

        // Per-file failure breakdown
        let mut file_failures: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for f in &failures {
            // Format is "FAIL: filename.dat:linenum\n..."
            let after_prefix = &f[6..]; // skip "FAIL: "
            if let Some(colon) = after_prefix.find(':') {
                let name = &after_prefix[..colon];
                *file_failures.entry(name.to_string()).or_default() += 1;
            }
        }
        let mut sorted: Vec<_> = file_failures.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        eprintln!("\nFailures by file:");
        for (name, count) in &sorted {
            eprintln!("  {count:3} {name}");
        }

        let rate = passed as f64 / (passed + failed) as f64 * 100.0;
        eprintln!("\nConformance rate: {rate:.1}% ({passed}/{} non-skipped tests)", passed + failed);

        // Require 100% conformance — any regression is a failure.
        assert!(
            rate >= 100.0,
            "Conformance rate {rate:.1}% — expected 100%. {failed} test(s) failed."
        );
    }
}
