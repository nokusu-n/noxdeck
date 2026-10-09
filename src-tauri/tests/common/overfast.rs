//! Local HTTP stand-in; captures full requests, serves only invented profiles.
use std::{collections::HashMap, io::{Read, Write}, net::TcpListener, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, thread, time::Duration};

#[derive(Clone)]
pub struct Reply { pub status: u16, pub body: String, pub delay: Duration }
impl Reply {
    pub fn json(body: &str) -> Self { Self { status: 200, body: body.into(), delay: Duration::ZERO } }
}
pub struct Stub {
    pub url: String,
    pub replies: Arc<Mutex<HashMap<String, Reply>>>,
    pub requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}
impl Stub {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let stub = Self { url: format!("http://{}", listener.local_addr().unwrap()), replies: Default::default(), requests: Default::default(), stop: Arc::new(AtomicBool::new(false)) };
        let (replies, requests, stop) = (stub.replies.clone(), stub.requests.clone(), stub.stop.clone());
        thread::spawn(move || while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let (replies, requests) = (replies.clone(), requests.clone());
                    thread::spawn(move || {
                        stream.set_nonblocking(false).unwrap();
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let mut request = Vec::new();
                        let mut byte = [0];
                        while stream.read_exact(&mut byte).is_ok() {
                            request.push(byte[0]);
                            if request.ends_with(b"\r\n\r\n") { break; }
                        }
                        let headers = String::from_utf8_lossy(&request);
                        let length = headers.lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().ok()).flatten()
                        }).unwrap_or(0);
                        let mut body = vec![0; length.min(65536)];
                        let _ = stream.read_exact(&mut body);
                        request.extend(body);
                        let request = String::from_utf8_lossy(&request).into_owned();
                        let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
                        requests.lock().unwrap().push(request);
                        let reply = replies.lock().unwrap().get(&path).cloned().unwrap_or(Reply { status:404, body:String::new(), delay:Duration::ZERO });
                        thread::sleep(reply.delay);
                        let _ = write!(stream, "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.status, reply.body.len(), reply.body);
                    });
                }
                Err(_) => thread::sleep(Duration::from_millis(2)),
            }
        });
        stub
    }
    pub fn set(&self, tag: &str, reply: Reply) { self.replies.lock().unwrap().insert(format!("/players/{}/summary", tag.replace('#', "-")), reply); }
    pub fn count(&self) -> usize { self.requests.lock().unwrap().len() }
}
impl Drop for Stub { fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); } }
