use std::fs;
use std::io::{self, BufRead, BufReader};

#[derive(Debug, Default)]
pub struct Netstat {
    pub ip_ext: IpExt,
}

#[derive(Debug, Default)]
pub struct IpExt {
    pub in_octets: Option<f64>,
    pub out_octets: Option<f64>,
}

impl Netstat {
    pub fn read(pid: i32) -> io::Result<Netstat> {
        let filename = format!("/proc/{}/net/netstat", pid);
        let proc_netstat = read_from_file(&filename)?;
        Ok(proc_netstat)
    }
}
fn read_from_file(path: &str) -> io::Result<Netstat> {
    let data = fs::read(path)?;
    parse_proc_netstat(&data[..], path)
}

fn parse_proc_netstat<R: io::Read>(reader: R, file_name: &str) -> io::Result<Netstat> {
    let mut proc_netstat = Netstat::default();
    let reader = BufReader::new(reader);
    let mut lines = reader.lines();

    while let Some(header_line) = lines.next() {
        let header = header_line?;
        let name_parts: Vec<&str> = header.split_whitespace().collect();

        let value_line = match lines.next() {
            Some(l) => l?,
            None => break,
        };
        let value_parts: Vec<&str> = value_line.split_whitespace().collect();

        let protocol = name_parts[0].trim_end_matches(':');
        if name_parts.len() != value_parts.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("mismatch field count in {}: {}", file_name, protocol),
            ));
        }

        for i in 1..name_parts.len() {
            let value: f64 = value_parts[i].parse().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid value in {}: {}", file_name, e),
                )
            })?;
            let key = name_parts[i];
            match protocol {
                "IpExt" => match key {
                    "InOctets" => proc_netstat.ip_ext.in_octets = Some(value),
                    "OutOctets" => proc_netstat.ip_ext.out_octets = Some(value),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    Ok(proc_netstat)
}
