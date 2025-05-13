pub use facet;

#[macro_export]
macro_rules! plait {
    // Process first crate in the list, then recurse
    (
        with crates {
            $( #[ $($crateattr:tt)* ] )?
            $cratename:ident
            $($rest_crates:tt)*
        }
        $($rest:tt)*
    ) => {
        // Process this crate
        $crate::plait! {
            with crate {
                $( #[ $($crateattr)* ] )?
                $cratename
            }
            $($rest)*
        }

        // Recurse with remaining crates
        $crate::plait! {
            with crates {
                $($rest_crates)*
            }
            $($rest)*
        }
    };

    // Base case: empty crates list
    (
        with crates { }
        $($rest:tt)*
    ) => {
        $crate::plait!($($rest)*);
    };

    // Generic impl with following declarations
    (
       with crate {
           $( #[ $($crateattr:tt)* ] )?
           $cratename:ident
       }

       $(#[$attr:meta])*
       $vis:vis struct $name:ident => &$ref_name:ident;

       $($rest:tt)+
    ) => {
        $crate::plait! {
            @impl_internal {
                $( #[ $($crateattr)* ] )?
                $cratename
            }
            $(#[$attr])*
            $vis struct $name => &$ref_name;
        }

        $crate::plait! {
            with crate {
                $( #[ $($crateattr)* ] )?
                $cratename
            }
            $($rest)+
        }
    };

    // Generic impl for last declaration
    (
       with crate {
           $( #[ $($crateattr:tt)* ] )?
           $cratename:ident
       }

       $(#[$attr:meta])*
       $vis:vis struct $name:ident => &$ref_name:ident;
    ) => {
        $crate::plait! {
            @impl_internal {
                $( #[ $($crateattr)* ] )?
                $cratename
            }
            $(#[$attr])*
            $vis struct $name => &$ref_name;
        }
    };

    // Internal rule for impls
    (
        @impl_internal {
            $( #[ $($crateattr:tt)* ] )?
            serde
        }
        $(#[$attr:meta])*
        $vis:vis struct $name:ident => &$ref_name:ident;
    ) => {
        #[automatically_derived]
        $( #[ $($crateattr)* ] )?
        impl ::serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                self.0.serialize(serializer)
            }
        }

        #[automatically_derived]
        $( #[ $($crateattr)* ] )?
        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                <::std::string::String as ::serde::Deserialize>::deserialize(deserializer).map(Self::new)
            }
        }
    };

    // Internal rule for rusqlite impls
    (
        @impl_internal {
            $( #[ $($crateattr:tt)* ] )?
            rusqlite
        }
        $(#[$attr:meta])*
        $vis:vis struct $name:ident => &$ref_name:ident;
    ) => {
        #[automatically_derived]
        $( #[ $($crateattr)* ] )?
        impl ::rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> ::rusqlite::Result<::rusqlite::types::ToSqlOutput<'_>> {
                Ok(::rusqlite::types::ToSqlOutput::Borrowed(
                    ::rusqlite::types::ValueRef::Text(self.as_str().as_bytes())
                ))
            }
        }

        #[automatically_derived]
        $( #[ $($crateattr)* ] )?
        impl ::rusqlite::types::FromSql for $name {
            fn column_result(value: ::rusqlite::types::ValueRef<'_>) -> ::rusqlite::types::FromSqlResult<Self> {
                value.as_str().map(|s| Self::new(s.to_owned()))
            }
        }

        #[automatically_derived]
        $( #[ $($crateattr)* ] )?
        impl ::rusqlite::types::ToSql for $ref_name {
            fn to_sql(&self) -> ::rusqlite::Result<::rusqlite::types::ToSqlOutput<'_>> {
                Ok(::rusqlite::types::ToSqlOutput::Borrowed(
                    ::rusqlite::types::ValueRef::Text(self.as_str().as_bytes())
                ))
            }
        }
    };

    // Internal rule for minijinja impls
    (
        @impl_internal {
            $( #[ $($crateattr:tt)* ] )?
            minijinja
        }
        $(#[$attr:meta])*
        $vis:vis struct $name:ident => &$ref_name:ident;
    ) => {
        #[automatically_derived]
        $( #[ $($crateattr)* ] )?
        impl<'a> ::minijinja::value::ArgType<'a> for $name {
            type Output = Self;

            fn from_value(value: Option<&'a ::minijinja::value::Value>) -> Result<Self, ::minijinja::Error> {
                match value {
                    Some(value) => {
                        if value.is_kwargs() {
                            Err(::minijinja::Error::new(::minijinja::ErrorKind::InvalidOperation, "cannot convert kwargs to string"))
                        } else {
                            Ok(value.to_string().into())
                        }
                    }
                    None => Err(::minijinja::Error::from(::minijinja::ErrorKind::MissingArgument))
                }
            }

            fn from_value_owned(value: ::minijinja::value::Value) -> Result<Self, ::minijinja::Error> {
                if value.is_kwargs() {
                    Err(::minijinja::Error::new(::minijinja::ErrorKind::InvalidOperation, "cannot convert kwargs to string"))
                } else {
                    Ok(value.to_string().into())
                }
            }
        }
    };

    // Entry point
    (
        $(
            $(#[$attr:meta])*
            $vis:vis struct $name:ident => &$ref_name:ident;
        )+
    ) => {
        $(
            #[derive(Clone, Hash, PartialEq, Eq, $crate::facet::Facet)]
            #[repr(transparent)]
            #[facet(transparent)]
            $(#[$attr])*
            pub struct $name(String);

            #[automatically_derived]
            impl $name {
                #[doc = "Constructs a new $name"]
                #[inline]
                pub const fn new(raw: String) -> Self {
                    Self(raw)
                }
                #[inline]
                #[doc = "Constructs a new $name from a static reference"]
                #[track_caller]
                pub fn from_static(raw: &'static str) -> Self {
                    ::std::borrow::ToOwned::to_owned($ref_name::from_static(raw))
                }
                #[doc = "Converts this `$name` into a [`Box<$ref_name>`]\n\nThis will drop any excess capacity."]
                #[allow(unsafe_code)]
                #[inline]
                pub fn into_boxed_ref(self) -> ::std::boxed::Box<$ref_name> {
                    // SAFETY: `$ref_name` is `#[repr(transparent)]` around a single `str` field
                    let box_str = ::std::string::String::from(self.0).into_boxed_str();
                    unsafe {
                        ::std::boxed::Box::from_raw(::std::boxed::Box::into_raw(box_str) as *mut $ref_name)
                    }
                }
                #[doc = "Unwraps the underlying [`String`] value"]
                #[inline]
                pub fn take(self) -> String {
                    self.0
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<&'_ $ref_name> for $name {
                #[inline]
                fn from(s: &$ref_name) -> Self {
                    ::std::borrow::ToOwned::to_owned(s)
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<$name> for ::std::string::String {
                #[inline]
                fn from(s: $name) -> Self {
                    ::std::convert::From::from(s.0)
                }
            }
            #[automatically_derived]
            impl ::std::borrow::Borrow<$ref_name> for $name {
                #[inline]
                fn borrow(&self) -> &$ref_name {
                    ::std::ops::Deref::deref(self)
                }
            }
            #[automatically_derived]
            impl ::std::convert::AsRef<$ref_name> for $name {
                #[inline]
                fn as_ref(&self) -> &$ref_name {
                    ::std::ops::Deref::deref(self)
                }
            }
            #[automatically_derived]
            impl ::std::convert::AsRef<str> for $name {
                #[inline]
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<$name> for ::std::boxed::Box<$ref_name> {
                #[inline]
                fn from(r: $name) -> Self {
                    r.into_boxed_ref()
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<::std::boxed::Box<$ref_name>> for $name {
                #[inline]
                fn from(r: ::std::boxed::Box<$ref_name>) -> Self {
                    r.into_owned()
                }
            }
            #[automatically_derived]
            impl<'a> ::std::convert::From<::std::borrow::Cow<'a, $ref_name>> for $name {
                #[inline]
                fn from(r: ::std::borrow::Cow<'a, $ref_name>) -> Self {
                    match r {
                        ::std::borrow::Cow::Borrowed(b) => ::std::borrow::ToOwned::to_owned(b),
                        ::std::borrow::Cow::Owned(o) => o,
                    }
                }
            }
            #[automatically_derived]
            impl<'a> ::std::convert::From<$name> for ::std::borrow::Cow<'a, $ref_name> {
                #[inline]
                fn from(owned: $name) -> Self {
                    ::std::borrow::Cow::Owned(owned)
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<::std::string::String> for $name {
                #[inline]
                fn from(s: ::std::string::String) -> Self {
                    Self::new(From::from(s))
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<&'_ str> for $name {
                #[inline]
                fn from(s: &str) -> Self {
                    Self::new(::std::convert::From::from(s))
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<::std::boxed::Box<str>> for $name {
                #[inline]
                fn from(s: ::std::boxed::Box<str>) -> Self {
                    Self::new(::std::convert::From::from(s))
                }
            }
            #[automatically_derived]
            impl ::std::str::FromStr for $name {
                type Err = ::std::convert::Infallible;
                #[inline]
                fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                    ::std::result::Result::Ok(::std::convert::From::from(s))
                }
            }
            #[automatically_derived]
            impl ::std::borrow::Borrow<str> for $name {
                #[inline]
                fn borrow(&self) -> &str {
                    self.as_str()
                }
            }
            #[automatically_derived]
            impl ::std::ops::Deref for $name {
                type Target = $ref_name;
                #[inline]
                fn deref(&self) -> &Self::Target {
                    $ref_name::from_str(::std::convert::AsRef::as_ref(&self.0))
                }
            }
            #[automatically_derived]
            impl ::std::fmt::Debug for $name {
                #[inline]
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    <$ref_name as ::std::fmt::Debug>::fmt(::std::ops::Deref::deref(self), f)
                }
            }
            #[automatically_derived]
            impl ::std::fmt::Display for $name {
                #[inline]
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    <$ref_name as ::std::fmt::Display>::fmt(::std::ops::Deref::deref(self), f)
                }
            }
            #[automatically_derived]
            impl ::std::cmp::Ord for $name {
                #[inline]
                fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                    ::std::cmp::Ord::cmp(&self.0, &other.0)
                }
            }
            #[automatically_derived]
            impl ::std::cmp::PartialOrd for $name {
                #[inline]
                fn partial_cmp(&self, other: &Self) -> ::std::option::Option<::std::cmp::Ordering> {
                    ::std::cmp::PartialOrd::partial_cmp(&self.0, &other.0)
                }
            }
            #[repr(transparent)]
            #[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
            $(#[$attr])*
            pub struct $ref_name(str);

            #[automatically_derived]
            impl $ref_name {
                #[allow(unsafe_code)]
                #[inline]
                #[doc = "Transparently reinterprets the string slice as a strongly-typed $ref_name"]
                pub const fn from_str(raw: &str) -> &Self {
                    let ptr: *const str = raw;

                    // SAFETY: `$ref_name` is `#[repr(transparent)]` around a single `str` field, so a `*const str` can be safely reinterpreted as a `*const $ref_name`
                    unsafe { &*(ptr as *const Self) }
                }
                #[inline]
                #[doc = "Transparently reinterprets the static string slice as a strongly-typed $ref_name"]
                #[track_caller]
                pub const fn from_static(raw: &'static str) -> &'static Self {
                    Self::from_str(raw)
                }
                #[allow(unsafe_code)]
                #[inline]
                #[doc = "Converts a [`Box<$ref_name>`] into a [`$name`] without copying or allocating"]
                pub fn into_owned(self: ::std::boxed::Box<$ref_name>) -> $name {
                    // "SAFETY: `$ref_name` is `#[repr(transparent)]` around a single `str` field, so a `*mut str` can be safely reinterpreted as a `*mut $ref_name`"
                    let raw = ::std::boxed::Box::into_raw(self);
                    let boxed = unsafe { ::std::boxed::Box::from_raw(raw as *mut str) };
                    $name::new(::std::convert::From::from(boxed))
                }
                #[doc = r" Provides access to the underlying value as a string slice."]
                #[inline]
                pub const fn as_str(&self) -> &str {
                    &self.0
                }
            }
            #[automatically_derived]
            impl ::std::borrow::ToOwned for $ref_name {
                type Owned = $name;
                #[inline]
                fn to_owned(&self) -> Self::Owned {
                    $name(self.0.into())
                }
            }
            #[automatically_derived]
            impl ::std::cmp::PartialEq<$ref_name> for $name {
                #[inline]
                fn eq(&self, other: &$ref_name) -> bool {
                    self.as_str() == other.as_str()
                }
            }
            #[automatically_derived]
            impl ::std::cmp::PartialEq<$name> for $ref_name {
                #[inline]
                fn eq(&self, other: &$name) -> bool {
                    self.as_str() == other.as_str()
                }
            }
            #[automatically_derived]
            impl ::std::cmp::PartialEq<&'_ $ref_name> for $name {
                #[inline]
                fn eq(&self, other: &&$ref_name) -> bool {
                    self.as_str() == other.as_str()
                }
            }
            #[automatically_derived]
            impl ::std::cmp::PartialEq<$name> for &'_ $ref_name {
                #[inline]
                fn eq(&self, other: &$name) -> bool {
                    self.as_str() == other.as_str()
                }
            }
            #[automatically_derived]
            impl<'a> ::std::convert::From<&'a str> for &'a $ref_name {
                #[inline]
                fn from(s: &'a str) -> &'a $ref_name {
                    $ref_name::from_str(s)
                }
            }
            #[automatically_derived]
            impl ::std::borrow::Borrow<str> for $ref_name {
                #[inline]
                fn borrow(&self) -> &str {
                    &self.0
                }
            }
            #[automatically_derived]
            impl ::std::convert::AsRef<str> for $ref_name {
                #[inline]
                fn as_ref(&self) -> &str {
                    &self.0
                }
            }
            #[automatically_derived]
            impl<'a> ::std::convert::From<&'a $ref_name> for ::std::borrow::Cow<'a, $ref_name> {
                #[inline]
                fn from(r: &'a $ref_name) -> Self {
                    ::std::borrow::Cow::Borrowed(r)
                }
            }
            #[automatically_derived]
            impl<'a, 'b: 'a> ::std::convert::From<&'a ::std::borrow::Cow<'b, $ref_name>>
                for &'a $ref_name
            {
                #[inline]
                fn from(r: &'a ::std::borrow::Cow<'b, $ref_name>) -> &'a $ref_name {
                    ::std::borrow::Borrow::borrow(r)
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<&'_ $ref_name> for ::std::rc::Rc<$ref_name> {
                #[allow(unsafe_code)]
                #[inline]
                fn from(r: &'_ $ref_name) -> Self {
                    // SAFETY: `$ref_name` is `#[repr(transparent)]` around a single `str` field, so a `*const str` can be safely reinterpreted as a `*const $ref_name`
                    let rc = ::std::rc::Rc::<str>::from(r.as_str());
                    unsafe { ::std::rc::Rc::from_raw(::std::rc::Rc::into_raw(rc) as *const $ref_name) }
                }
            }
            #[automatically_derived]
            impl ::std::convert::From<&'_ $ref_name> for ::std::sync::Arc<$ref_name> {
                #[allow(unsafe_code)]
                #[inline]
                fn from(r: &'_ $ref_name) -> Self {
                    // SAFETY: `$ref_name` is `#[repr(transparent)]` around a single `str` field, so a `*const str` can be safely reinterpreted as a `*const $ref_name`
                    let arc = ::std::sync::Arc::<str>::from(r.as_str());
                    unsafe {
                        ::std::sync::Arc::from_raw(::std::sync::Arc::into_raw(arc) as *const $ref_name)
                    }
                }
            }
            #[automatically_derived]
            impl ::std::fmt::Debug for $ref_name {
                #[inline]
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    <str as ::std::fmt::Debug>::fmt(&self.0, f)
                }
            }
            #[automatically_derived]
            impl ::std::fmt::Display for $ref_name {
                #[inline]
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    <str as ::std::fmt::Display>::fmt(&self.0, f)
                }
            }
        )+
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    plait! {
        with crates {
            #[doc = "yay"]
            serde
            #[doc = "yahoo"]
            rusqlite
        }

        pub struct Test => &TestRef;
    }
}
