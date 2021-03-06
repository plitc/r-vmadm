
//! Wrapper around zfs commands

use std::error::Error;
use std::process::{Command, Stdio};
use errors::GenericError;
use std::io::Read;
use std::io::copy;


#[derive(Debug)]
/// Basic information about a ZFS dataset
pub struct ZFSEntry {
    name: String,
    used: u64,
    avail: u64,
    refer: u64,
    mountpoint: String,
}

// /// reads the zfs datasets in a pool
// pub fn list(pool: &str) -> Result<Vec<ZFSEntry>, Box<Error>> {
//     debug!("Listing ZFS datasets"; "pool" => pool);
//     let output = Command::new("zfs")
//         .args(&["list", "-p", "-H", "-r", "-d1", pool])
//         .output()
//         .expect("zfs list failed");
//     let reply = String::from_utf8_lossy(&output.stdout);
//     let mut res = Vec::new();

//     //Ok(reply.split('\n').filter(|x| *x != "").map(&deconstruct_entry).collect())
//     for line in reply.split('\n').filter(|x| *x != "") {
//         let entry = deconstruct_entry(line)?;
//         res.push(entry)
//     }
//     Ok(res)
// }


pub fn receive<R>(dataset: &str, mut reader: &mut R) -> Result<i32, Box<Error>>
    where
    R: Read,
{
    let mut rec = Command::new("zfs")
        .args(&["receive", dataset])
        .stdin(Stdio::piped())
        .spawn().unwrap();
    let mut stdin = rec.stdin.take().unwrap();
    copy(&mut reader, &mut stdin)?;
    Ok(0)
}

/// checks weather a dataset exists or not
pub fn is_present(dataset: &str) -> bool {
    get(dataset).is_ok()
}

/// reads the zfs datasets in a pool
fn get(dataset: &str) -> Result<ZFSEntry, Box<Error>> {
    debug!("Reading ZFS dataset"; "dataset" => dataset);
    let output = Command::new("zfs")
        .args(&["list", "-p", "-H", dataset])
        .output()
        .expect("zfs list failed");
    if output.status.success() {
        let reply = String::from_utf8_lossy(&output.stdout).to_string();
        deconstruct_entry(reply.as_str())
    } else {
        Err(GenericError::bx("Failed to get dataset"))
    }
}

/// reads the zfs datasets in a pool
pub fn origin(dataset: &str) -> Result<String, Box<Error>> {
    debug!("Fetching ZFS origin"; "dataset" => dataset);
    let output = Command::new("zfs")
        .args(&["get", "-p", "-H", "origin", dataset])
        .output()
        .expect("zfs list failed");
    if output.status.success() {
        let out = String::from_utf8_lossy(&output.stdout).to_string();
        let mut reply = out.split('\t');
        reply.next().ok_or_else(
            || GenericError::bx("NAME field missing"),
        )?;
        reply.next().ok_or_else(
            || GenericError::bx("PROPERTY field missing"),
        )?;
        let origin = reply.next().ok_or_else(
            || GenericError::bx("PROPERTY field missing"),
        )?;
        Ok(String::from(origin))
    } else {
        Err(GenericError::bx("Failed to get dataset"))
    }
}

// /// create a zfs datasets in a pool
// pub fn create(dataset: &str) -> Result<i32, Box<Error>> {
//     debug!("Creating ZFS dataset"; "dataset" => dataset);
//     let output = Command::new("zfs")
//         .args(&["create", dataset])
//         .output()
//         .expect("zfs create failed");
//     if output.status.success() {
//         Ok(0)
//     } else {
//         Err(GenericError::bx("Failed create dataset"))
//     }
// }

/// sets the quota in gigabyte for a dataset
pub fn quota(dataset: &str, quota: u64) -> Result<u32, Box<Error>> {
    let dataset = String::from(dataset);
    let mut set = String::from("quota=");
    set.push_str(quota.to_string().as_str());
    set.push('G');

    let args = vec!["set", set.as_str(), dataset.as_str()];
    debug!("Setting ZFS snapshot"; "dataset" => dataset.clone(), "quota" => set.clone(),
           "args" => args.clone().join(" "));
    let output = Command::new("zfs").args(args).output().expect(
        "zfs set failed",
    );
    if output.status.success() {
        Ok(0)
    } else {
        Err(GenericError::bx("Failed set quota"))
    }
}

/// create a zfs snapshot of a dataset
pub fn snapshot(dataset: &str, snapshot: &str) -> Result<String, Box<Error>> {
    let mut snap = String::from(dataset);
    snap.push('@');
    snap.push_str(snapshot);
    let args = vec!["snapshot", snap.as_str()];
    debug!("Creating ZFS snapshot"; "dataset" => dataset, "snapshot" => snapshot,
    "args" => args.clone().join(" "));
    let output = Command::new("zfs").args(args).output().expect(
        "zfs snapshot failed",
    );
    if output.status.success() {
        Ok(snap.clone())
    } else {
        Err(GenericError::bx("Failed create snapshot"))
    }
}

/// clones a zfs snapshot
pub fn clone(snapshot: &str, dataset: &str) -> Result<i32, Box<Error>> {
    debug!("Cloning ZFS snapshot"; "snapshot" => snapshot, "dataset" => dataset);
    let output = Command::new("zfs")
        .args(&["clone", snapshot, dataset])
        .output()
        .expect("zfs clone failed");
    if output.status.success() {
        Ok(0)
    } else {
        Err(GenericError::bx("Failed to clone dataset"))
    }
}

/// destroy the zfs datasets in a pool
pub fn destroy(dataset: &str) -> Result<i32, Box<Error>> {
    debug!("deleteing ZFS dataset"; "dataset" => dataset);
    let output = Command::new("zfs")
        .args(&["destroy", dataset])
        .output()
        .expect("zfs destroy failed");
    if output.status.success() {
        Ok(0)
    } else {
        Err(GenericError::bx("Failed destroy dataset"))
    }
}

// /// deconstructs a line from zfs list into an `ZFSEntry`.
fn deconstruct_entry(line: &str) -> Result<ZFSEntry, Box<Error>> {
    let mut parts = line.split('\t');
    let name = parts.next().ok_or_else(
        || GenericError::bx("NAME field missing"),
    )?;
    let n0 = parts.next().ok_or_else(
        || GenericError::bx("USED field missing"),
    )?;
    let used: u64 = n0.parse()?;
    let n1 = parts.next().ok_or_else(
        || GenericError::bx("AVAIL field missing"),
    )?;
    let avail: u64 = n1.parse()?;
    let n2 = parts.next().ok_or_else(
        || GenericError::bx("REFER field missing"),
    )?;
    let refer: u64 = n2.parse()?;
    let mountpoint = parts.next().ok_or_else(
        || GenericError::bx("MOUNTPOINT field missing"),
    )?;

    Ok(ZFSEntry {
        name: String::from(name),
        used: used,
        avail: avail,
        refer: refer,
        mountpoint: String::from(mountpoint),
    })
}
