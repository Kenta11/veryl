use crate::OptIpxact;
use crate::context::Context;
use crate::ipxact::{IpxactBuilder, TopLevelItem};
use log::info;
use miette::{IntoDiagnostic, Result, WrapErr};
use std::collections::BTreeMap;
use std::fs;
use veryl_analyzer::symbol::SymbolKind;
use veryl_analyzer::{Analyzer, symbol_table};
use veryl_metadata::Metadata;
use veryl_parser::Parser;
use veryl_parser::resource_table;

pub struct CmdIpxact {
    opt: OptIpxact,
}

impl CmdIpxact {
    pub fn new(opt: OptIpxact) -> Self {
        Self { opt }
    }

    pub fn exec(&self, metadata: &mut Metadata) -> Result<bool> {
        let paths = metadata.paths(&self.opt.files, true, true)?;

        let mut contexts = Vec::new();

        for path in &paths {
            info!("Processing file ({})", path.src.to_string_lossy());

            let input = fs::read_to_string(&path.src)
                .into_diagnostic()
                .wrap_err("")?;
            let parser = Parser::parse(&input, &path.src)?;
            let analyzer = Analyzer::new(metadata);
            analyzer.analyze_pass1(&path.prj, &parser.veryl);

            let context = Context::new(path.clone(), input, parser, analyzer)?;
            contexts.push(context);
        }

        Analyzer::analyze_post_pass1();

        let mut analyzer_context = veryl_analyzer::Context::default();
        let mut ir = veryl_analyzer::ir::Ir::default();
        for context in &contexts {
            let path = &context.path;
            context.analyzer.analyze_pass2(
                &path.prj,
                &context.parser.veryl,
                &mut analyzer_context,
                Some(&mut ir),
            );
        }

        Analyzer::analyze_post_pass2(&ir);

        // Collect the public modules belonging to this project. IP-XACT
        // describes component interfaces, so only modules (not interfaces or
        // packages) are emitted.
        let mut modules = BTreeMap::new();

        for symbol in symbol_table::get_all() {
            let text = resource_table::get_str_value(symbol.token.text).unwrap();
            let file_name = text.clone();
            let symbol = symbol.clone();
            if format!("{}", symbol.namespace) == metadata.project.name && symbol.public {
                if let SymbolKind::Module(_) = &symbol.kind {
                    let item = TopLevelItem { file_name, symbol };
                    modules.insert(text, item);
                }
            }
        }

        let modules: Vec<_> = modules.into_values().collect();

        let builder = IpxactBuilder::new(metadata, modules)?;
        builder.build()?;

        Ok(true)
    }
}
