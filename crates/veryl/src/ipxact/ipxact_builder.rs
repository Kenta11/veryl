use log::{info, warn};
use miette::{IntoDiagnostic, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use veryl_analyzer::symbol::{Direction, ParameterKind, Symbol, SymbolKind};
use veryl_metadata::{IpxactVersion, Metadata};
use veryl_parser::Stringifier;
use veryl_parser::veryl_grammar_trait::Expression;
use veryl_parser::veryl_walker::VerylWalker;

/// Render a Veryl expression (parameter value, port width, array size) back to
/// its source-like string form using the parser's `Stringifier`.
fn expr_to_string(expr: &Expression) -> String {
    let mut stringifier = Stringifier::new();
    stringifier.expression(expr);
    stringifier.as_str().to_string()
}

/// A public module collected from the project, to be emitted as one IP-XACT
/// component description.
pub struct TopLevelItem {
    /// Base file name (the Veryl module name) used for the output `*.xml` file.
    pub file_name: String,
    /// The resolved module symbol.
    pub symbol: Symbol,
}

/// Generates IEEE 1685 (IP-XACT) component descriptions for the public modules
/// of a project.
pub struct IpxactBuilder {
    metadata: Metadata,
    modules: Vec<TopLevelItem>,
}

impl IpxactBuilder {
    pub fn new(metadata: &Metadata, modules: Vec<TopLevelItem>) -> Result<Self> {
        Ok(Self {
            metadata: metadata.clone(),
            modules,
        })
    }

    /// Emit one `<module>.xml` file per public module into the configured
    /// output directory (`[ipxact] path`, default `ipxact/`).
    pub fn build(&self) -> Result<()> {
        let path = &self.metadata.ipxact.path;
        fs::create_dir_all(path).into_diagnostic()?;

        for item in &self.modules {
            let Some(xml) = self.build_component(&item.file_name, &item.symbol) else {
                continue;
            };

            let mut file = PathBuf::from(path);
            file.push(format!("{}.xml", item.file_name));
            info!("Generating file ({})", file.to_string_lossy());
            fs::write(&file, xml).into_diagnostic()?;
        }

        Ok(())
    }

    /// Build the IP-XACT component XML for a single module symbol.
    ///
    /// Returns `None` when the symbol is not a module (defensive; the caller
    /// only passes modules).
    fn build_component(&self, name: &str, symbol: &Symbol) -> Option<String> {
        let SymbolKind::Module(property) = &symbol.kind else {
            return None;
        };

        let version = self.metadata.ipxact.version;
        let p = version.prefix();

        let vendor = self.metadata.ipxact_vendor();
        let library = self.metadata.ipxact_library();
        let component_version = self.metadata.ipxact_component_version();
        let sv_module_name = self.sv_module_name(name);

        // `param`-kind parameters become module parameters (HDL view) and
        // top-level component parameters. `const`/localparam are HDL-internal
        // and intentionally excluded.
        let parameters: Vec<(String, String)> = property
            .parameters
            .iter()
            .filter(|x| matches!(x.property().kind, ParameterKind::Param))
            .map(|x| {
                let name = x.name.to_string();
                let value = x
                    .property()
                    .value
                    .as_ref()
                    .map(expr_to_string)
                    .unwrap_or_default();
                (name, value.trim().to_string())
            })
            .collect();

        let mut out = String::new();
        let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        let _ = writeln!(
            out,
            r#"<{p}:component xmlns:{p}="{ns}" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="{loc}">"#,
            ns = version.namespace(),
            loc = version.schema_location(),
        );
        let _ = writeln!(out, "  <{p}:vendor>{}</{p}:vendor>", escape(&vendor));
        let _ = writeln!(out, "  <{p}:library>{}</{p}:library>", escape(&library));
        let _ = writeln!(out, "  <{p}:name>{}</{p}:name>", escape(name));
        let _ = writeln!(
            out,
            "  <{p}:version>{}</{p}:version>",
            escape(&component_version)
        );

        self.build_model(&mut out, version, &sv_module_name, property, &parameters);

        // 2014/2022 expose HDL parameters again at the component level so they
        // can be referenced from bus/abstraction definitions.
        if version != IpxactVersion::V2009 && !parameters.is_empty() {
            let _ = writeln!(out, "  <{p}:parameters>");
            for (pname, value) in &parameters {
                let _ = writeln!(
                    out,
                    r#"    <{p}:parameter parameterId="{id}">"#,
                    id = escape(pname)
                );
                let _ = writeln!(out, "      <{p}:name>{}</{p}:name>", escape(pname));
                let _ = writeln!(out, "      <{p}:value>{}</{p}:value>", escape(value));
                let _ = writeln!(out, "    </{p}:parameter>");
            }
            let _ = writeln!(out, "  </{p}:parameters>");
        }

        let _ = writeln!(out, "</{p}:component>");
        Some(out)
    }

    /// Emit the `<model>` element. 1685-2014/2022 nest HDL details in a
    /// `componentInstantiation`; 1685-2009 places `modelName`/`modelParameters`
    /// directly inside the view/model.
    fn build_model(
        &self,
        out: &mut String,
        version: IpxactVersion,
        sv_module_name: &str,
        property: &veryl_analyzer::symbol::ModuleProperty,
        parameters: &[(String, String)],
    ) {
        let p = version.prefix();
        let _ = writeln!(out, "  <{p}:model>");

        if version == IpxactVersion::V2009 {
            // --- 1685-2009 layout ---
            let _ = writeln!(out, "    <{p}:views>");
            let _ = writeln!(out, "      <{p}:view>");
            let _ = writeln!(out, "        <{p}:name>rtl</{p}:name>");
            let _ = writeln!(out, "        <{p}:envIdentifier>::</{p}:envIdentifier>");
            let _ = writeln!(out, "        <{p}:language>verilog</{p}:language>");
            let _ = writeln!(
                out,
                "        <{p}:modelName>{}</{p}:modelName>",
                escape(sv_module_name)
            );
            let _ = writeln!(out, "      </{p}:view>");
            let _ = writeln!(out, "    </{p}:views>");

            self.build_ports(out, version, property);

            if !parameters.is_empty() {
                let _ = writeln!(out, "    <{p}:modelParameters>");
                for (pname, value) in parameters {
                    let _ = writeln!(out, "      <{p}:modelParameter>");
                    let _ = writeln!(out, "        <{p}:name>{}</{p}:name>", escape(pname));
                    let _ = writeln!(
                        out,
                        r#"        <{p}:value {p}:resolve="user" {p}:id="{id}">{v}</{p}:value>"#,
                        id = escape(pname),
                        v = escape(value)
                    );
                    let _ = writeln!(out, "      </{p}:modelParameter>");
                }
                let _ = writeln!(out, "    </{p}:modelParameters>");
            }
        } else {
            // --- 1685-2014 / 1685-2022 layout ---
            let _ = writeln!(out, "    <{p}:views>");
            let _ = writeln!(out, "      <{p}:view>");
            let _ = writeln!(out, "        <{p}:name>rtl</{p}:name>");
            let _ = writeln!(
                out,
                "        <{p}:componentInstantiationRef>rtl</{p}:componentInstantiationRef>"
            );
            let _ = writeln!(out, "      </{p}:view>");
            let _ = writeln!(out, "    </{p}:views>");

            let _ = writeln!(out, "    <{p}:instantiations>");
            let _ = writeln!(out, "      <{p}:componentInstantiation>");
            let _ = writeln!(out, "        <{p}:name>rtl</{p}:name>");
            let _ = writeln!(
                out,
                "        <{p}:moduleName>{}</{p}:moduleName>",
                escape(sv_module_name)
            );
            if !parameters.is_empty() {
                let _ = writeln!(out, "        <{p}:moduleParameters>");
                for (pname, value) in parameters {
                    let _ = writeln!(
                        out,
                        r#"          <{p}:moduleParameter parameterId="{id}">"#,
                        id = escape(pname)
                    );
                    let _ = writeln!(out, "            <{p}:name>{}</{p}:name>", escape(pname));
                    let _ = writeln!(out, "            <{p}:value>{}</{p}:value>", escape(value));
                    let _ = writeln!(out, "          </{p}:moduleParameter>");
                }
                let _ = writeln!(out, "        </{p}:moduleParameters>");
            }
            let _ = writeln!(out, "      </{p}:componentInstantiation>");
            let _ = writeln!(out, "    </{p}:instantiations>");

            self.build_ports(out, version, property);
        }

        let _ = writeln!(out, "  </{p}:model>");
    }

    /// Emit the `<ports>` element. Only wire-style ports (`input`/`output`/
    /// `inout`) are representable; interface/modport ports are skipped with a
    /// warning since IP-XACT models those via bus interfaces instead.
    fn build_ports(
        &self,
        out: &mut String,
        version: IpxactVersion,
        property: &veryl_analyzer::symbol::ModuleProperty,
    ) {
        let p = version.prefix();
        let _ = writeln!(out, "    <{p}:ports>");

        for port in &property.ports {
            let prop = port.property();
            let dir = match prop.direction {
                Direction::Input | Direction::Import => "in",
                Direction::Output => "out",
                Direction::Inout => "inout",
                Direction::Interface | Direction::Modport => {
                    warn!(
                        "Skipping port ({}) with interface/modport direction; not representable as an IP-XACT wire port",
                        port.name()
                    );
                    continue;
                }
            };

            let _ = writeln!(out, "      <{p}:port>");
            let _ = writeln!(
                out,
                "        <{p}:name>{}</{p}:name>",
                escape(&port.name().to_string())
            );
            let _ = writeln!(out, "        <{p}:wire>");
            let _ = writeln!(out, "          <{p}:direction>{dir}</{p}:direction>");

            // Packed widths -> vector(s). An empty width list is a scalar wire.
            let widths: Vec<String> = prop.r#type.width.iter().map(expr_to_string).collect();
            self.build_vectors(out, version, &widths);

            let _ = writeln!(out, "        </{p}:wire>");

            // Unpacked dimensions -> arrays (2014/2022 only).
            if version != IpxactVersion::V2009 {
                let arrays: Vec<String> =
                    prop.r#type.array.iter().map(expr_to_string).collect();
                if !arrays.is_empty() {
                    let _ = writeln!(out, "        <{p}:arrays>");
                    for a in &arrays {
                        let _ = writeln!(out, "          <{p}:array>");
                        let _ = writeln!(
                            out,
                            "            <{p}:left>{}</{p}:left>",
                            escape(&left_bound(a))
                        );
                        let _ = writeln!(out, "            <{p}:right>0</{p}:right>");
                        let _ = writeln!(out, "          </{p}:array>");
                    }
                    let _ = writeln!(out, "        </{p}:arrays>");
                }
            }

            let _ = writeln!(out, "      </{p}:port>");
        }

        let _ = writeln!(out, "    </{p}:ports>");
    }

    /// Emit the packed-dimension vector(s) for a wire port.
    fn build_vectors(&self, out: &mut String, version: IpxactVersion, widths: &[String]) {
        let p = version.prefix();
        if widths.is_empty() {
            return;
        }

        if version == IpxactVersion::V2009 {
            // 1685-2009 has no `vectors` wrapper and supports a single vector.
            // Combine when the dimensions are all constant; otherwise fall back
            // to the first (most-significant) dimension.
            let left = combined_left_bound(widths);
            let _ = writeln!(out, "          <{p}:vector>");
            let _ = writeln!(out, "            <{p}:left>{}</{p}:left>", escape(&left));
            let _ = writeln!(out, "            <{p}:right>0</{p}:right>");
            let _ = writeln!(out, "          </{p}:vector>");
        } else {
            let _ = writeln!(out, "          <{p}:vectors>");
            for w in widths {
                let _ = writeln!(out, "            <{p}:vector>");
                let _ = writeln!(
                    out,
                    "              <{p}:left>{}</{p}:left>",
                    escape(&left_bound(w))
                );
                let _ = writeln!(out, "              <{p}:right>0</{p}:right>");
                let _ = writeln!(out, "            </{p}:vector>");
            }
            let _ = writeln!(out, "          </{p}:vectors>");
        }
    }

    /// The HDL (SystemVerilog) module name as emitted by `veryl build`.
    ///
    /// Veryl prefixes the project name unless `[build] omit_project_prefix` is
    /// set. The IP-XACT component `name` keeps the logical Veryl name, while
    /// `moduleName` must match the generated HDL so tools can elaborate it.
    fn sv_module_name(&self, name: &str) -> String {
        if self.metadata.build.omit_project_prefix {
            name.to_string()
        } else {
            format!("{}_{}", self.metadata.project.name, name)
        }
    }
}

/// Compute the `left` bound (`width - 1`) for a single dimension expression.
///
/// Constant widths are folded to a literal; parametric widths are emitted as an
/// IP-XACT expression that references the parameter by name (parentheses are
/// added when the source text is not a bare identifier/number to keep `- 1`
/// precedence correct).
fn left_bound(expr_text: &str) -> String {
    let t = expr_text.trim();
    if let Ok(n) = t.parse::<u128>() {
        return n.saturating_sub(1).to_string();
    }
    let bare = !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if bare {
        format!("{t}-1")
    } else {
        format!("({t})-1")
    }
}

/// Combine multiple packed dimensions into a single `left` bound for the
/// 1685-2009 schema (which permits only one vector). Constant dimensions are
/// multiplied; if any dimension is non-constant, the most-significant dimension
/// is used as a best-effort approximation.
fn combined_left_bound(widths: &[String]) -> String {
    let mut product: u128 = 1;
    let mut all_const = true;
    for w in widths {
        if let Ok(n) = w.trim().parse::<u128>() {
            product = product.saturating_mul(n);
        } else {
            all_const = false;
            break;
        }
    }
    if all_const {
        product.saturating_sub(1).to_string()
    } else {
        left_bound(&widths[0])
    }
}

/// Escape the five XML predefined entities for use in element text/attributes.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_bound_constant_is_folded() {
        assert_eq!(left_bound("8"), "7");
        assert_eq!(left_bound("1"), "0");
        assert_eq!(left_bound("0"), "0");
    }

    #[test]
    fn left_bound_parametric_keeps_expression() {
        assert_eq!(left_bound("WIDTH"), "WIDTH-1");
        assert_eq!(left_bound("WIDTH + 4"), "(WIDTH + 4)-1");
    }

    #[test]
    fn combined_left_bound_multiplies_constants() {
        let dims = vec!["4".to_string(), "8".to_string()];
        assert_eq!(combined_left_bound(&dims), "31");
    }

    #[test]
    fn combined_left_bound_falls_back_on_parametric() {
        let dims = vec!["WIDTH".to_string(), "8".to_string()];
        assert_eq!(combined_left_bound(&dims), "WIDTH-1");
    }

    #[test]
    fn escape_handles_xml_entities() {
        assert_eq!(escape("a<b&c>\"d'"), "a&lt;b&amp;c&gt;&quot;d&apos;");
        assert_eq!(escape("plain"), "plain");
    }
}
