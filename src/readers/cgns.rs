//! CGNS reader via system `libcgns` when available at build time.
//!
//! Public API only ([CGNS Mid-Level Library](https://cgns.github.io/CGNS_docs_current/midlevel/)).
//! When `has_libcgns` is not set (no headers / libs), returns a clear error that
//! documents how to enable the binding.
//!
//! Enable / install:
//! - Debian/Ubuntu: `sudo apt install libcgns-dev`
//! - Or set `CGNS_DIR=/path/to/prefix` and build with `--features cgns`
//! - Optional: `FLOWPLOT_FORCE_CGNS=1 cargo build --features cgns`

use std::path::Path;

use crate::dataset::Dataset;
use crate::error::Result;
use crate::readers::Reader;

#[cfg(not(has_libcgns))]
use crate::error::Error;

/// CGNS loader (real FFI when `has_libcgns`, otherwise a documented stub).
pub struct CgnsReader;

impl Reader for CgnsReader {
    fn format_name(&self) -> &'static str {
        "CGNS"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        #[cfg(has_libcgns)]
        {
            ffi::read_cgns(path)
        }
        #[cfg(not(has_libcgns))]
        {
            let _ = path;
            Err(Error::unimplemented(
                "CGNS",
                "system libcgns was not found at build time. Install libcgns-dev \
                 (headers + libcgns), then rebuild with `cargo build --features cgns` \
                 (or set CGNS_DIR / FLOWPLOT_FORCE_CGNS=1). Uses public Mid-Level API only: \
                 cg_open, cg_nbases, cg_nzones, cg_zone_read, cg_coord_read, cg_field_read. \
                 .cgz: decompress to .cgns first if cg_open rejects the path.",
            ))
        }
    }
}

#[cfg(has_libcgns)]
mod ffi {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::path::Path;

    use crate::dataset::{
        Dataset, Location, OrderedDims, Topology, VariableData, Zone,
    };
    use crate::error::{Error, Result};

    // Ubuntu libcgns 3.4: CG_BUILD_64BIT=0 → cgsize_t = int
    type Cgsize = c_int;

    const CG_MODE_READ: c_int = 0;
    const CG_OK: c_int = 0;
    const REAL_DOUBLE: c_int = 4; // DataType_t::RealDouble
    const STRUCTURED: c_int = 2;
    const VERTEX: c_int = 2;
    const CELL_CENTER: c_int = 3;

    extern "C" {
        fn cg_open(filename: *const c_char, mode: c_int, fn_: *mut c_int) -> c_int;
        fn cg_close(fn_: c_int) -> c_int;
        fn cg_nbases(fn_: c_int, nbases: *mut c_int) -> c_int;
        fn cg_nzones(fn_: c_int, B: c_int, nzones: *mut c_int) -> c_int;
        fn cg_zone_type(fn_: c_int, B: c_int, Z: c_int, type_: *mut c_int) -> c_int;
        fn cg_zone_read(
            fn_: c_int,
            B: c_int,
            Z: c_int,
            zonename: *mut c_char,
            size: *mut Cgsize,
        ) -> c_int;
        fn cg_ncoords(fn_: c_int, B: c_int, Z: c_int, ncoords: *mut c_int) -> c_int;
        fn cg_coord_info(
            fn_: c_int,
            B: c_int,
            Z: c_int,
            C: c_int,
            data_type: *mut c_int,
            coordname: *mut c_char,
        ) -> c_int;
        fn cg_coord_read(
            fn_: c_int,
            B: c_int,
            Z: c_int,
            coordname: *const c_char,
            type_: c_int,
            rmin: *const Cgsize,
            rmax: *const Cgsize,
            coord_array: *mut c_void,
        ) -> c_int;
        fn cg_nsols(fn_: c_int, B: c_int, Z: c_int, nsols: *mut c_int) -> c_int;
        fn cg_sol_info(
            fn_: c_int,
            B: c_int,
            Z: c_int,
            S: c_int,
            solname: *mut c_char,
            location: *mut c_int,
        ) -> c_int;
        fn cg_nfields(fn_: c_int, B: c_int, Z: c_int, S: c_int, nfields: *mut c_int) -> c_int;
        fn cg_field_info(
            fn_: c_int,
            B: c_int,
            Z: c_int,
            S: c_int,
            F: c_int,
            data_type: *mut c_int,
            fieldname: *mut c_char,
        ) -> c_int;
        fn cg_field_read(
            fn_: c_int,
            B: c_int,
            Z: c_int,
            S: c_int,
            fieldname: *const c_char,
            type_: c_int,
            rmin: *const Cgsize,
            rmax: *const Cgsize,
            field_ptr: *mut c_void,
        ) -> c_int;
        fn cg_get_error() -> *const c_char;
    }

    fn cgns_err(path: &Path, op: &str) -> Error {
        let msg = unsafe {
            let p = cg_get_error();
            if p.is_null() {
                format!("{op} failed")
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        };
        Error::parse("CGNS", format!("{}: {msg}", path.display()))
    }

    fn check(path: &Path, op: &str, code: c_int) -> Result<()> {
        if code == CG_OK {
            Ok(())
        } else {
            Err(cgns_err(path, op))
        }
    }

    fn c_name(buf: &[c_char]) -> String {
        let bytes: Vec<u8> = buf
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn read_cgns(path: &Path) -> Result<Dataset> {
        let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
            Error::parse("CGNS", format!("path contains NUL: {}", path.display()))
        })?;
        let mut fn_ = 0;
        unsafe {
            check(path, "cg_open", cg_open(c_path.as_ptr(), CG_MODE_READ, &mut fn_))?;
        }

        let result = (|| {
            let mut nbases = 0;
            unsafe {
                check(path, "cg_nbases", cg_nbases(fn_, &mut nbases))?;
            }
            if nbases < 1 {
                return Err(Error::parse("CGNS", "file has no bases"));
            }

            let mut dataset = Dataset::new();
            dataset.title = Some(path.file_name().and_then(|s| s.to_str()).unwrap_or("CGNS").into());

            // Read first base only in this slice (typical single-base CFD files).
            let b = 1;
            let mut nzones = 0;
            unsafe {
                check(path, "cg_nzones", cg_nzones(fn_, b, &mut nzones))?;
            }

            for z in 1..=nzones {
                let mut ztype = 0;
                unsafe {
                    check(path, "cg_zone_type", cg_zone_type(fn_, b, z, &mut ztype))?;
                }
                if ztype != STRUCTURED {
                    return Err(Error::parse(
                        "CGNS",
                        format!("zone {z}: only Structured zones are loaded in this build (type={ztype})"),
                    ));
                }

                let mut zname = [0i8; 33];
                let mut size = [0 as Cgsize; 9];
                unsafe {
                    check(
                        path,
                        "cg_zone_read",
                        cg_zone_read(fn_, b, z, zname.as_mut_ptr(), size.as_mut_ptr()),
                    )?;
                }
                let name = c_name(&zname);
                let ni = size[0].max(1) as usize;
                let nj = size[1].max(1) as usize;
                let nk = size[2].max(1) as usize;
                let nnodes = ni * nj * nk;

                let dims = if nk > 1 {
                    OrderedDims::Ijk {
                        i: ni,
                        j: nj,
                        k: nk,
                    }
                } else if nj > 1 {
                    OrderedDims::Ij { i: ni, j: nj }
                } else {
                    OrderedDims::I { i: ni }
                };

                let mut ncoords = 0;
                unsafe {
                    check(path, "cg_ncoords", cg_ncoords(fn_, b, z, &mut ncoords))?;
                }

                let mut var_names: Vec<String> = Vec::new();
                let mut fields: Vec<VariableData> = Vec::new();

                let rmin = [1 as Cgsize, 1, 1];
                let rmax = [ni as Cgsize, nj as Cgsize, nk as Cgsize];

                for c in 1..=ncoords {
                    let mut dtype = 0;
                    let mut cname = [0i8; 33];
                    unsafe {
                        check(
                            path,
                            "cg_coord_info",
                            cg_coord_info(fn_, b, z, c, &mut dtype, cname.as_mut_ptr()),
                        )?;
                    }
                    let coord_name = c_name(&cname);
                    let short = match coord_name.as_str() {
                        "CoordinateX" => "X".into(),
                        "CoordinateY" => "Y".into(),
                        "CoordinateZ" => "Z".into(),
                        other => other.to_string(),
                    };
                    let c_coord = CString::new(coord_name).unwrap();
                    let mut buf = vec![0f64; nnodes];
                    unsafe {
                        check(
                            path,
                            "cg_coord_read",
                            cg_coord_read(
                                fn_,
                                b,
                                z,
                                c_coord.as_ptr(),
                                REAL_DOUBLE,
                                rmin.as_ptr(),
                                rmax.as_ptr(),
                                buf.as_mut_ptr() as *mut c_void,
                            ),
                        )?;
                    }
                    var_names.push(short);
                    fields.push(VariableData {
                        location: Location::Nodal,
                        values: buf,
                    });
                }

                let mut nsols = 0;
                unsafe {
                    check(path, "cg_nsols", cg_nsols(fn_, b, z, &mut nsols))?;
                }
                for s in 1..=nsols {
                    let mut solname = [0i8; 33];
                    let mut loc = 0;
                    unsafe {
                        check(
                            path,
                            "cg_sol_info",
                            cg_sol_info(fn_, b, z, s, solname.as_mut_ptr(), &mut loc),
                        )?;
                    }
                    let location = if loc == CELL_CENTER {
                        Location::CellCentered
                    } else {
                        Location::Nodal
                    };
                    let (count, rmin_s, rmax_s) = if location == Location::CellCentered {
                        let ci = (ni.saturating_sub(1)).max(1);
                        let cj = (nj.saturating_sub(1)).max(1);
                        let ck = if nk > 1 {
                            (nk.saturating_sub(1)).max(1)
                        } else {
                            1
                        };
                        (
                            ci * cj * ck,
                            [1 as Cgsize, 1, 1],
                            [ci as Cgsize, cj as Cgsize, ck as Cgsize],
                        )
                    } else {
                        (nnodes, rmin, rmax)
                    };
                    let _ = VERTEX; // documented location enum

                    let mut nfields = 0;
                    unsafe {
                        check(
                            path,
                            "cg_nfields",
                            cg_nfields(fn_, b, z, s, &mut nfields),
                        )?;
                    }
                    for f in 1..=nfields {
                        let mut dtype = 0;
                        let mut fname = [0i8; 33];
                        unsafe {
                            check(
                                path,
                                "cg_field_info",
                                cg_field_info(
                                    fn_,
                                    b,
                                    z,
                                    s,
                                    f,
                                    &mut dtype,
                                    fname.as_mut_ptr(),
                                ),
                            )?;
                        }
                        let field_name = c_name(&fname);
                        let c_field = CString::new(field_name.clone()).unwrap();
                        let mut buf = vec![0f64; count];
                        unsafe {
                            check(
                                path,
                                "cg_field_read",
                                cg_field_read(
                                    fn_,
                                    b,
                                    z,
                                    s,
                                    c_field.as_ptr(),
                                    REAL_DOUBLE,
                                    rmin_s.as_ptr(),
                                    rmax_s.as_ptr(),
                                    buf.as_mut_ptr() as *mut c_void,
                                ),
                            )?;
                        }
                        var_names.push(field_name);
                        fields.push(VariableData {
                            location,
                            values: buf,
                        });
                    }
                }

                // Merge variable names across zones (first zone defines order).
                if dataset.variables.is_empty() {
                    dataset.variables = var_names;
                } else if dataset.variables != var_names {
                    // Pad / align by appending new names.
                    for (idx, name) in var_names.iter().enumerate() {
                        if idx >= dataset.variables.len() {
                            dataset.variables.push(name.clone());
                        }
                    }
                }

                dataset.zones.push(Zone {
                    name,
                    topology: Topology::Ordered(dims),
                    fields,
                });
                dataset.zone_meta.push(Default::default());
            }

            if dataset.zones.is_empty() {
                return Err(Error::parse("CGNS", "no zones found"));
            }
            Ok(dataset)
        })();

        unsafe {
            let _ = cg_close(fn_);
        }
        result
    }
}
