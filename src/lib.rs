#![allow(unused_imports)]
#![allow(dead_code)]
use ::ipld_collections::{Hamt, List};
use ::libipld::{
    cache::{Cache, IpldCache},
    cbor::{DagCbor, DagCborCodec},
    cid::Cid,
    error::Result,
    ipld::Ipld,
    multihash::Code,
    prelude::{Decode, Encode},
    store::{Store, StoreParams},
    DagCbor,
};
use ::std::collections::HashMap;
use ::std::path::{Component, Path, PathBuf};
use ::std::pin::Pin;
use ::std::ptr;

// only supports utf8 strings

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct Error;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fs {
    path: PathBuf,
    cwd: *mut Dir,
    root: Box<Dir>,
}
impl Default for Fs {
    fn default() -> Fs {
        let mut fs = Fs {
            path: PathBuf::from("/"),
            cwd: ptr::null::<Dir>() as *mut _,
            root: Box::new(Dir::new()),
        };
        fs.cwd = &*fs.root as *const _ as *mut _;
        fs
    }
}
fn reduce(unreduced: PathBuf) -> PathBuf {
    use Component::{CurDir, Normal, ParentDir, RootDir};
    let mut reduced = PathBuf::new();
    for (counter, comp) in unreduced.components().enumerate() {
        match comp {
            CurDir if counter == 0 => {
                let _ = reduced.push(comp);
            }
            RootDir => {
                reduced.push(comp);
            }
            ParentDir => {
                let copy = reduced.clone();
                let last = copy.components().rev().next();
                match last {
                    None => {
                        reduced.push(ParentDir);
                    }
                    Some(CurDir) => {
                        reduced.push(ParentDir);
                    }
                    Some(RootDir) => {
                        reduced.push(RootDir);
                    }
                    Some(ParentDir) => {
                        reduced.push(ParentDir);
                    }
                    Some(Normal(_)) => {
                        reduced.pop();
                    }
                    _ => unreachable!(),
                };
            }
            Normal(_) => {
                reduced.push(comp);
            }
            _ => {}
        }
    }
    reduced
}

impl Fs {
    fn split(&mut self) -> (&mut *mut Dir, &mut Box<Dir>) {
        (&mut self.cwd, &mut self.root)
    }
    fn new() -> Fs {
        Fs::default()
    }
    fn cd(&mut self, path: &str) -> Result<(), Error> {
        let unreduced = self.path.join(path);
        let reduced = reduce(unreduced);
        let mut dir = &*self.root;
        for comp in reduced.components() {
            use Component::{Normal, RootDir};
            dir = match comp {
                RootDir => &*self.root,
                Normal(name) => dir
                    .members
                    .get(name.to_str().expect("name came from utf-8"))
                    .ok_or(Error)?
                    .as_dir_ref()
                    .ok_or(Error)?,
                _ => dir,
            }
        }
        self.cwd = &*dir as *const _ as *mut _;
        Ok(())
    }
    fn mkdir(&mut self, name: &str) -> Result<(), Error> {
        let unreduced = self.path.join(name);
        let reduced = reduce(unreduced);
        let dir_name = reduced
            .file_name()
            .ok_or(Error)?
            .to_str()
            .expect("must be utf-8");
        if dir_name.is_empty() {
            return Err(Error);
        }
        let save_ptr = self.cwd;
        let save_path = self.path.clone();
        let parent = reduced.parent().unwrap();
        match self.cd(parent.to_str().unwrap()) {
            Ok(_) => {}
            Err(_) => {
                self.cwd = save_ptr;
                self.path = save_path;
            }
        }
        let dir = AnyFile::dir();
        // self.cwd must point to a valid Dir
        let dir_ref = unsafe { &mut *self.cwd };
        if dir_ref.members.get(dir_name).is_some() {
            return Err(Error);
        }
        let _ = dir_ref.members.insert(name.to_string(), dir);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AnyFile {
    File(File),
    Dir(Box<Dir>),
}

impl AnyFile {
    fn as_dir_ref(&self) -> Option<&Dir> {
        use AnyFile::*;
        match self {
            Dir(dir) => Some(dir),
            _ => None,
        }
    }
    fn dir() -> Self {
        AnyFile::Dir(Box::new(Dir::new()))
    }
}

// only supports utf8 names
#[derive(Clone, Debug, PartialEq, Eq)]
struct Dir {
    members: HashMap<String, AnyFile>,
}

impl Dir {
    fn new() -> Dir {
        Dir {
            members: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DirEnt {
    content: AnyFile,
    attribs: Attribs,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct File {
    name: String,
    content: Box<u8>,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct Attribs {
    mtime: i32,
    posix: u16,
    sticky: bool,
    setgid: bool,
    setuid: bool,
    uid: u32,
    gid: u32,
}

mod tests {
    use super::*;
    #[test]
    fn test_reduce() {
        let reduced_path = |path: &str| reduce(PathBuf::from(path));
        let as_path = |path: &str| PathBuf::from(path);
        assert_eq!(reduced_path("//.."), as_path("/"));
        assert_eq!(reduced_path(".//.."), as_path("./.."));
        assert_eq!(reduced_path("./a/.."), as_path("."));
        assert_eq!(reduced_path("./a/"), as_path("./a"));
        assert_eq!(reduced_path("./a/b/.."), as_path("./a"));
        assert_eq!(reduced_path("./a//b//.."), as_path("./a"));
        assert_eq!(reduced_path("./a//b//../"), as_path("./a"));
        assert_eq!(reduced_path("./a/b/c/d/e/"), as_path("./a/b/c/d/e"));
        assert_eq!(reduced_path("/////a/b/c/"), as_path("/a/b/c"));
        assert_eq!(reduced_path(""), as_path(""));
        assert_eq!(reduced_path("/"), as_path("/"));
        assert_eq!(reduced_path("."), as_path("."));
        assert_eq!(reduced_path(".."), as_path(".."));
    }
}
