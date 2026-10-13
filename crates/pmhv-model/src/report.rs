use std::error::Error;

/// Formats an error with all its sources, outermost first: `outer: middle: root cause`.
pub fn error_chain(error: &(dyn Error + 'static)) -> String {
    std::iter::successors(Some(error), |error| (*error).source())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}
