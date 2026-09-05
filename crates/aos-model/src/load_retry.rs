//! Bounded retry shared by the real loader and fault-injection tests.
pub fn load_with_cpu_fallback<T>(
    accelerated: bool,
    mut attempt: impl FnMut(bool) -> Result<T, String>,
) -> Result<(T, bool), String> {
    match attempt(false) {
        Ok(value) => Ok((value, false)),
        Err(first) if accelerated => {
            eprintln!("[modeld] chargement accéléré échoué; tentative CPU");
            attempt(true)
                .map(|value| (value, true))
                .map_err(|second| format!("chargement accéléré: {first}; repli CPU: {second}"))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_is_bounded_and_only_for_accelerated_loads() {
        let mut calls = vec![];
        let result = load_with_cpu_fallback(true, |cpu| {
            calls.push(cpu);
            if cpu {
                Ok(42)
            } else {
                Err("context allocation".into())
            }
        })
        .unwrap();
        assert_eq!(result, (42, true));
        assert_eq!(calls, [false, true]);
        calls.clear();
        assert!(load_with_cpu_fallback::<()>(false, |cpu| {
            calls.push(cpu);
            Err("bad file".into())
        })
        .is_err());
        assert_eq!(calls, [false]);
        let error =
            load_with_cpu_fallback::<()>(true, |cpu| Err(if cpu { "cpu" } else { "gpu" }.into()))
                .unwrap_err();
        assert!(error.contains("gpu") && error.contains("cpu"));
    }
}
