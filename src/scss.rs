use crate::error::AppError;
use std::fs;
use std::path::Path;

const SCSS_ENTRY: &str = "static/scss/style.scss";
const CSS_OUTPUT: &str = "static/styles/style.css";

pub(crate) fn compile_scss() -> Result<(), AppError> {
    compile_scss_from(Path::new(SCSS_ENTRY), Path::new(CSS_OUTPUT))
}

pub(crate) fn compile_scss_from(entry: &Path, output: &Path) -> Result<(), AppError> {
    let css =
        grass::from_path(entry, &grass::Options::default()).map_err(|source| AppError::Scss {
            path: entry.display().to_string(),
            source,
        })?;

    fs::write(output, css).map_err(|source| AppError::CssWrite {
        path: output.display().to_string(),
        source,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn compiles_valid_scss_to_css() {
        let dir = tempdir().unwrap();
        let entry = dir.path().join("style.scss");
        let output = dir.path().join("style.css");

        fs::write(
            &entry,
            "$brand: #ff0000;\n.card { color: $brand; .title { font-weight: bold; } }",
        )
        .unwrap();

        compile_scss_from(&entry, &output).unwrap();

        let css = fs::read_to_string(&output).unwrap();
        assert!(css.contains(".card"));
        assert!(css.contains("color: red") || css.contains("#ff0000"));
        // nesting should be flattened
        assert!(css.contains(".card .title"));
        // variables should not leak into output
        assert!(!css.contains("$brand"));
    }

    #[test]
    fn resolves_imports_from_partials() {
        let dir = tempdir().unwrap();
        let entry = dir.path().join("style.scss");
        let output = dir.path().join("style.css");

        fs::write(dir.path().join("_vars.scss"), "$gap: 4px;").unwrap();
        fs::write(&entry, "@use 'vars';\n.box { margin: vars.$gap; }").unwrap();

        compile_scss_from(&entry, &output).unwrap();

        let css = fs::read_to_string(&output).unwrap();
        assert!(css.contains("margin: 4px"));
    }

    #[test]
    fn invalid_scss_returns_scss_error_with_path() {
        let dir = tempdir().unwrap();
        let entry = dir.path().join("broken.scss");
        let output = dir.path().join("out.css");

        fs::write(&entry, ".a { color: $undefined; }").unwrap();

        let err = compile_scss_from(&entry, &output).unwrap_err();

        match &err {
            AppError::Scss { path, .. } => {
                assert!(path.ends_with("broken.scss"));
            }
            other => panic!("expected AppError::Scss, got {other:?}"),
        }
        assert!(err.to_string().contains("broken.scss"));
        // nothing should be written on failure
        assert!(!output.exists());
    }

    #[test]
    fn missing_entry_file_returns_scss_error() {
        let dir = tempdir().unwrap();
        let entry = dir.path().join("does_not_exist.scss");
        let output = dir.path().join("out.css");

        let err = compile_scss_from(&entry, &output).unwrap_err();

        assert!(matches!(err, AppError::Scss { .. }));
    }

    #[test]
    fn unwritable_output_returns_css_write_error() {
        let dir = tempdir().unwrap();
        let entry = dir.path().join("style.scss");
        let output = dir.path().join("no_such_dir").join("style.css");

        fs::write(&entry, ".a { color: red; }").unwrap();

        let err = compile_scss_from(&entry, &output).unwrap_err();

        match err {
            AppError::CssWrite { path, .. } => assert!(path.ends_with("style.css")),
            other => panic!("expected AppError::CssWrite, got {other:?}"),
        }
    }
}

/*
fn watch_scss() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (tx, rx) = channel();

    let mut watcher = notify::recommended_watcher(tx)?;

    watcher.watch(Path::new("static/scss"), RecursiveMode::Recursive)?;

    for result in rx {
        match result {
            Ok(event) => {
                if event
                    .paths
                    .iter()
                    .any(|path| path.extension().is_some_and(|ext| ext == "scss"))
                {
                    tracing::debug!("SCSS changed");

                    if let Err(err) = compile_scss() {
                        tracing::error!("SCSS compilation failed: {err}");
                    }
                }
            }
            Err(err) => {
                tracing::error!("SCSS watcher error: {err}");
            }
        }
    }

    Ok(())
}
*/
