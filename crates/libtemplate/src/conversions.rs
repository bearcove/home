pub(crate) trait ToMinijinjaError {
    fn to_minijinja_error(&self) -> minijinja::Error;
}

impl ToMinijinjaError for noteyre::BS {
    fn to_minijinja_error(&self) -> minijinja::Error {
        minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, format!("{self}"))
    }
}

impl ToMinijinjaError for conflux::RevisionError {
    fn to_minijinja_error(&self) -> minijinja::Error {
        minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, self.to_string())
    }
}

impl ToMinijinjaError for closest::ClosestError {
    fn to_minijinja_error(&self) -> minijinja::Error {
        minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, self.to_string())
    }
}

pub(crate) trait ToMinijinaResult<T> {
    fn mj(self) -> Result<T, minijinja::Error>;
}

impl<T, E> ToMinijinaResult<T> for std::result::Result<T, E>
where
    E: ToMinijinjaError,
{
    fn mj(self) -> Result<T, minijinja::Error> {
        self.map_err(|e| e.to_minijinja_error())
    }
}
