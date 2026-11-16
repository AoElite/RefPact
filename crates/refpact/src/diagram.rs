//! The diagram file: a diagram that a test keeps current.

use std::path::Path;

use crate::core::DiagramFormat;
use crate::Failure;

fn update_requested() -> bool {
    std::env::var("REFPACT_DIAGRAM").is_ok_and(|value| value == "update")
}

/// Passes when the file holds the diagram of the rule that `draw` gives in the format of
/// the file. When the environment variable `REFPACT_DIAGRAM` is `update`, this writes the
/// diagram to a file that does not hold it, and passes.
pub(crate) fn verify(
    rule: &str,
    file: &Path,
    draw: impl FnOnce(DiagramFormat) -> Result<String, Failure>,
) -> Result<(), Failure> {
    let name = file.display().to_string();
    let failed = |message: String| Failure::Diagram { message };
    let Some(format) = DiagramFormat::of_file(&name) else {
        return Err(failed(format!(
            "the name of the diagram file {name} must end in .md, .mmd, .mermaid, .dot or .gv"
        )));
    };
    let text = draw(format)?;
    match std::fs::read_to_string(file) {
        // A checkout can give the file the line ends of Windows.
        Ok(found) if found.replace("\r\n", "\n") == text => Ok(()),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(failed(format!("cannot read {name}: {error}")))
        }
        // The file has another text, or is not there.
        _ if update_requested() => {
            // Several tests can check one file, and each writes it through the same partial file.
            let _guard = crate::baseline::writing();
            crate::baseline::write_file(file, &text)
                .map_err(|error| failed(format!("cannot write {name}: {error}")))
        }
        _ => Err(Failure::StaleDiagram {
            rule: rule.to_owned(),
            file: name,
        }),
    }
}
