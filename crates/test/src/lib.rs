#![no_std]
#![deny(rustdoc::broken_intra_doc_links)]

pub use flipperzero_test_macros::{tests, tests_runner};

/// The type of a Flipper Zero test function.
pub type TestFn = fn() -> TestResult;

/// The result type of a Flipper Zero test.
pub type TestResult = Result<(), TestFailure>;

/// A failure that occurred within a Flipper Zero test.
#[derive(Debug)]
pub enum TestFailure {
    AssertEq {
        left: &'static str,
        right: &'static str,
        msg: Option<&'static str>,
    },
    AssertNe {
        left: &'static str,
        right: &'static str,
        msg: Option<&'static str>,
    },
    Str(&'static str),
}

impl From<&'static str> for TestFailure {
    fn from(value: &'static str) -> Self {
        TestFailure::Str(value)
    }
}

impl ufmt::uDisplay for TestFailure {
    fn fmt<W>(&self, f: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        match self {
            TestFailure::AssertEq { left, right, msg } => {
                f.write_str("assertion failed: ")?;
                f.write_str(left)?;
                f.write_str(" == ")?;
                f.write_str(right)?;
                if let Some(msg) = msg {
                    f.write_str("\n")?;
                    f.write_str(msg)?;
                }
                Ok(())
            }
            TestFailure::AssertNe { left, right, msg } => {
                f.write_str("assertion failed: ")?;
                f.write_str(left)?;
                f.write_str(" != ")?;
                f.write_str(right)?;
                if let Some(msg) = msg {
                    f.write_str("\n")?;
                    f.write_str(msg)?;
                }
                Ok(())
            }
            TestFailure::Str(s) => f.write_str(s),
        }
    }
}

pub mod __macro_support {
    use core::ffi::{c_char, CStr};

    use flipperzero_sys as sys;
    use sys::furi::UnsafeRecord;

    use crate::TestFn;

    const RECORD_STORAGE: *const c_char = sys::c_string!("storage");

    pub struct Args<'a>(&'a str);

    impl<'a> Args<'a> {
        /// Parses test arguments from a raw C string.
        ///
        /// The total size of the raw C string must be smaller than `isize::MAX` **bytes**
        /// in memory due to calling the `slice::from_raw_parts` function.
        ///
        /// If the C string does not contain valid UTF-8, it is ignored and the test is
        /// run without arguments.
        ///
        /// # Safety
        ///
        /// * The memory pointed to by `ptr` must contain a valid nul terminator at the
        ///   end of the string.
        ///
        /// * `ptr` must be [valid] for reads of bytes up to and including the null
        ///   terminator. This means in particular that the entire memory range of the C
        ///   string must be contained within a single allocated object!
        ///
        /// * The memory referenced by the returned `CStr` must not be mutated for
        ///   the duration of lifetime `'a`.
        ///
        /// # Caveat
        ///
        /// The lifetime for the returned slice is inferred from its usage.
        ///
        /// [valid]: core::ptr#safety
        pub unsafe fn parse(args: *mut u8) -> Self {
            if args.is_null() {
                Args("")
            } else {
                let args_cstr = unsafe { CStr::from_ptr(args.cast_const().cast()) };
                let args = args_cstr.to_str().unwrap_or("");
                Args(args)
            }
        }

        fn filters(&self) -> Filters<'a> {
            Filters(Some(self.0))
        }
    }

    struct Filters<'a>(Option<&'a str>);

    impl<'a> Iterator for Filters<'a> {
        type Item = &'a str;

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                // Get the next token in the buffer.
                let buf = self.0.take()?;
                let token = match buf.split_once(' ') {
                    Some((next, rest)) => {
                        self.0 = Some(rest);
                        next
                    }
                    None => buf,
                };

                // If the token is not an option, it is a filter.
                if !token.starts_with('-') {
                    break Some(token);
                }
            }
        }
    }

    struct OutputFile(*mut sys::File);

    impl Drop for OutputFile {
        fn drop(&mut self) {
            unsafe { sys::storage_file_free(self.0) };
        }
    }

    impl OutputFile {
        fn new(storage: &UnsafeRecord<sys::Storage>) -> Self {
            let output_file = unsafe { sys::storage_file_alloc(storage.as_ptr()) };
            unsafe {
                sys::storage_file_open(
                    output_file,
                    sys::c_string!("/ext/flipperzero-rs-stdout"),
                    sys::FS_AccessMode_FSAM_WRITE,
                    sys::FS_OpenMode_FSOM_CREATE_ALWAYS,
                );
            }
            Self(output_file)
        }
    }

    impl ufmt::uWrite for OutputFile {
        type Error = i32;

        fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
            assert!(s.len() <= u16::MAX as usize);
            let mut buf = s.as_bytes();
            while !buf.is_empty() {
                let written = unsafe {
                    sys::storage_file_write(self.0, s.as_bytes().as_ptr().cast(), s.len() as u16)
                };
                if written == 0 {
                    return Err(1); // TODO
                }
                buf = &buf[written as usize..];
            }
            Ok(())
        }
    }

    pub fn run_tests(
        test_count: usize,
        tests: impl Iterator<Item = (&'static str, &'static str, TestFn)> + Clone,
        args: Args<'_>,
    ) -> Result<(), i32> {
        let storage: UnsafeRecord<sys::Storage> = unsafe { UnsafeRecord::open(RECORD_STORAGE) };
        let mut output_file = OutputFile::new(&storage);

        #[inline]
        fn filtered_out(filters: Filters<'_>, module: &str, name: &str) -> bool {
            let mut have_filters = false;
            let mut matched = false;
            for filter in filters {
                have_filters = true;
                matched |= module.contains(filter) || name.contains(filter);
            }
            have_filters && !matched
        }

        // Count the number of filtered tests.
        let mut filtered = 0;
        for (module, name, _) in tests.clone() {
            if filtered_out(args.filters(), module, name) {
                filtered += 1;
            }
        }

        ufmt::uwriteln!(output_file, "")?;
        ufmt::uwriteln!(output_file, "running {} tests", test_count - filtered)?;

        let heap_before = unsafe { sys::memmgr_get_free_heap() };
        let cycle_counter = unsafe { sys::furi_get_tick() };
        let mut failed = 0;
        for (module, name, test_fn) in tests {
            if filtered_out(args.filters(), module, name) {
                continue;
            }

            ufmt::uwrite!(output_file, "test {}::{} ... ", module, name)?;
            if let Err(e) = test_fn() {
                failed += 1;
                ufmt::uwriteln!(output_file, "FAILED")?;
                ufmt::uwriteln!(output_file, "")?;
                ufmt::uwriteln!(output_file, "---- {}::{} stdout ----", module, name)?;
                ufmt::uwriteln!(output_file, "{}", e)?;
                ufmt::uwriteln!(output_file, "")?;
            } else {
                ufmt::uwriteln!(output_file, "ok")?;
            };
        }
        let time_taken = unsafe { sys::furi_get_tick() } - cycle_counter;

        // Wait for tested services and apps to deallocate memory
        unsafe { sys::furi_delay_us(10_000) };
        let heap_after = unsafe { sys::memmgr_get_free_heap() };

        // Final Report
        ufmt::uwriteln!(output_file, "")?;
        ufmt::uwriteln!(
            output_file,
            "test result: {}. {} passed; {} failed; 0 ignored; 0 measured; {} filtered out; finished in {}ms",
            if failed == 0 { "ok" } else { "FAILED" },
            test_count - failed - filtered,
            failed,
            filtered,
            time_taken,
        )?;
        ufmt::uwriteln!(output_file, "leaked: {} bytes", heap_before - heap_after)?;
        ufmt::uwriteln!(output_file, "")?;

        if failed == 0 {
            Ok(())
        } else {
            Err(1)
        }
    }
}
