use futures;
use futures::Async;
use std;
use std::io::Result as IoResult;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tokio_io::{AsyncRead, AsyncWrite};

use std::fs::{File,OpenOptions};
use std::rc::Rc;

use super::{BoxedNewPeerFuture, Peer, Result};

use super::{once, Handle, Options, PeerConstructor, ProgramState, Specifier};

#[derive(Clone, Debug)]
pub struct ReadFile(pub PathBuf);
impl Specifier for ReadFile {
    fn construct(
        &self,
        _h: &Handle,
        _ps: &mut ProgramState,
        _opts: Rc<Options>,
    ) -> PeerConstructor {
        fn gp(p: &Path) -> Result<Peer> {
            let f = File::open(p)?;
            Ok(Peer::new(ReadFileWrapper(f), super::trivial_peer::DevNull))
        }
        once(Box::new(futures::future::result(gp(&self.0))) as BoxedNewPeerFuture)
    }
    specifier_boilerplate!(typ=Other noglobalstate singleconnect no_subspec);
}
specifier_class!(
    name=ReadFileClass, 
    target=ReadFile, 
    prefixes=["readfile:"], 
    arg_handling=into,
    help="TODO"
);

#[derive(Clone, Debug)]
pub struct WriteFile(pub PathBuf);
impl Specifier for WriteFile {
    fn construct(
        &self,
        _h: &Handle,
        _ps: &mut ProgramState,
        _opts: Rc<Options>,
    ) -> PeerConstructor {
        fn gp(p: &Path) -> Result<Peer> {
            let f = File::create(p)?;
            Ok(Peer::new(super::trivial_peer::DevNull, WriteFileWrapper(f)))
        }
        once(Box::new(futures::future::result(gp(&self.0))) as BoxedNewPeerFuture)
    }
    specifier_boilerplate!(typ=Other noglobalstate singleconnect no_subspec);
}
specifier_class!(
    name=WriteFileClass, 
    target=WriteFile, 
    prefixes=["writefile:"], 
    arg_handling=into,
    help="TODO"
);

#[derive(Clone, Debug)]
pub struct AppendFile(pub PathBuf);
impl Specifier for AppendFile {
    fn construct(
        &self,
        _h: &Handle,
        _ps: &mut ProgramState,
        _opts: Rc<Options>,
    ) -> PeerConstructor {
        fn gp(p: &Path) -> Result<Peer> {
            let f = OpenOptions::new().create(true).append(true).open(p)?;
            Ok(Peer::new(super::trivial_peer::DevNull, WriteFileWrapper(f)))
        }
        once(Box::new(futures::future::result(gp(&self.0))) as BoxedNewPeerFuture)
    }
    specifier_boilerplate!(typ=Other noglobalstate singleconnect no_subspec);
}
specifier_class!(
    name=AppendFileClass, 
    target=AppendFile, 
    prefixes=["appendfile:"], 
    arg_handling=into,
    help="TODO"
);

struct ReadFileWrapper(File);

impl AsyncRead for ReadFileWrapper {}
impl Read for ReadFileWrapper {
    fn read(&mut self, buf: &mut [u8]) -> std::result::Result<usize, std::io::Error> {
        self.0.read(buf)
    }
}

struct WriteFileWrapper(File);

impl AsyncWrite for WriteFileWrapper {
    fn shutdown(&mut self) -> futures::Poll<(), std::io::Error> {
        Ok(Async::Ready(()))
    }
}
impl Write for WriteFileWrapper {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> IoResult<()> {
        self.0.flush()
    }
}