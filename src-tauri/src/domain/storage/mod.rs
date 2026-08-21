use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::security::{redact, shell_escape};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// 描述远端块设备的稳定摘要；不会返回分区内容或敏感挂载凭据。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StorageDevice {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub filesystem: Option<String>,
    pub size_bytes: u64,
    pub readonly: bool,
    pub removable: bool,
    pub mountpoint: Option<String>,
    pub model: Option<String>,
}

/// 描述一条真实挂载点及其来自 df 的容量数据。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StorageMount {
    pub mountpoint: String,
    pub source: String,
    pub filesystem: String,
    pub options: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f64,
}

/// 描述 /etc/fstab 中的一条非注释配置；行号用于 UI 核对而不是删除凭据。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FstabEntry {
    pub line_number: u32,
    pub source: String,
    pub mountpoint: String,
    pub filesystem: String,
    pub options: String,
    pub dump: String,
    pub pass: String,
}

/// 块设备拓扑摘要，便于快速识别磁盘、分区、RAID 阵列与 LVM 卷。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorageTopology {
    pub disks: usize,
    pub partitions: usize,
    pub raid_arrays: usize,
    pub lvm_volumes: usize,
    pub other_devices: usize,
}

/// 返回磁盘、挂载点和 fstab 的真实远程快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSnapshot {
    pub devices: Vec<StorageDevice>,
    pub topology: StorageTopology,
    pub mounts: Vec<StorageMount>,
    pub fstab: Vec<FstabEntry>,
    pub warnings: Vec<String>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 描述一项需要 root/sudo 的挂载或 fstab 变更。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageActionInput {
    pub server_id: String,
    /// mount、unmount、add_fstab 或 remove_fstab。
    pub action: String,
    pub source: Option<String>,
    pub mountpoint: String,
    pub filesystem: Option<String>,
    pub options: Option<String>,
    #[serde(default)]
    pub dump: Option<String>,
    #[serde(default)]
    pub pass: Option<String>,
    pub confirmed: bool,
}

/// 返回挂载变更的受控摘要；远端 stderr 会在错误路径脱敏。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageActionResult {
    pub action: String,
    pub mountpoint: String,
    pub fstab_updated: bool,
    pub mounted: Option<bool>,
    pub output: String,
}

const STORAGE_PROBE_COMMAND: &str = r#"set +e
printf '__LSBLK__\n'
if command -v lsblk >/dev/null 2>&1; then lsblk -b -P -o NAME,PATH,TYPE,FSTYPE,SIZE,RO,RM,MOUNTPOINT,MODEL 2>/dev/null; fi
printf '__MOUNTS__\n'
if command -v findmnt >/dev/null 2>&1; then findmnt -rn -o TARGET,SOURCE,FSTYPE,OPTIONS 2>/dev/null; else cat /proc/mounts 2>/dev/null; fi
printf '__DF__\n'
df -B1 -P 2>/dev/null || true
printf '__FSTAB__\n'
if [ -r /etc/fstab ]; then awk 'BEGIN { OFS="\t" } /^[[:space:]]*#/ || NF < 4 { next } { printf "__FSTAB_ENTRY__\t%d\t%s\t%s\t%s\t%s\t%s\t%s\n", NR, $1, $2, $3, $4, ($5 == "" ? "0" : $5), ($6 == "" ? "0" : $6) }' /etc/fstab; fi
"#;

/// 读取远端真实存储信息，并在输出缺少 marker 时返回明确错误。
pub async fn snapshot(ssh: &SshConnectionManager, server_id: &str) -> AppResult<StorageSnapshot> {
    validate_server_id(server_id)?;
    let result = ssh
        .execute_system(server_id, STORAGE_PROBE_COMMAND, Duration::from_secs(30))
        .await?;
    if !result.stdout.contains("__LSBLK__") {
        return Err(AppError::new(
            "STORAGE_PROBE_FAILED",
            "storage",
            "无法读取远端磁盘与挂载信息",
        )
        .details(redact(&result.stderr))
        .for_server(server_id));
    }
    parse_snapshot(&result.stdout).map_err(|error| error.for_server(server_id))
}

/// 在用户确认后执行挂载、卸载或 fstab 变更，并返回重新读取前的受控结果。
pub async fn action(
    ssh: &SshConnectionManager,
    input: StorageActionInput,
) -> AppResult<StorageActionResult> {
    validate_action(&input)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "storage",
            "磁盘或 fstab 变更需要显式确认",
        )
        .for_server(input.server_id));
    }
    let (command, fstab_updated, expected_mounted) = build_action_command(&input)?;
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("STORAGE_ACTION_FAILED", "storage", "远端磁盘变更失败")
                .details(redact(&result.stderr))
                .for_server(input.server_id),
        );
    }
    Ok(StorageActionResult {
        action: input.action,
        mountpoint: input.mountpoint,
        fstab_updated,
        mounted: expected_mounted,
        output: redact(&result.stdout).chars().take(2_000).collect(),
    })
}

/// 解析固定 marker 区段并合并 df 容量数据，忽略不完整或未知行。
pub fn parse_snapshot(input: &str) -> AppResult<StorageSnapshot> {
    let sections = split_sections(input);
    let devices = parse_devices(
        sections
            .get("LSBLK")
            .map(String::as_str)
            .unwrap_or_default(),
    );
    let df = parse_df(sections.get("DF").map(String::as_str).unwrap_or_default());
    let mounts = parse_mounts(
        sections
            .get("MOUNTS")
            .map(String::as_str)
            .unwrap_or_default(),
        &df,
    );
    let fstab = parse_fstab(input);
    let mut warnings = Vec::new();
    if devices.is_empty() {
        warnings.push("远端没有返回可识别的 lsblk 设备；可能缺少 util-linux 权限".into());
    }
    if mounts.is_empty() {
        warnings.push("远端没有返回挂载点；请检查 findmnt/proc 权限".into());
    }
    let topology = compute_topology(&devices);
    Ok(StorageSnapshot {
        devices,
        topology,
        mounts,
        fstab,
        warnings,
        fetched_at: chrono::Utc::now(),
    })
}

/// 根据块设备类型汇总拓扑结构；RAID 类型以 `raid` 前缀识别。
fn compute_topology(devices: &[StorageDevice]) -> StorageTopology {
    let mut topology = StorageTopology {
        disks: 0,
        partitions: 0,
        raid_arrays: 0,
        lvm_volumes: 0,
        other_devices: 0,
    };
    for device in devices {
        match device.kind.as_str() {
            "disk" => topology.disks += 1,
            "part" => topology.partitions += 1,
            kind if kind.starts_with("raid") => topology.raid_arrays += 1,
            "lvm" => topology.lvm_volumes += 1,
            _ => topology.other_devices += 1,
        }
    }
    topology
}

/// 将 lsblk 的 KEY="VALUE" 行解析为设备摘要。
fn parse_devices(input: &str) -> Vec<StorageDevice> {
    input
        .lines()
        .filter_map(parse_lsblk_line)
        .filter(|device| !device.path.is_empty() && device.kind != "loop")
        .collect()
}

/// 解析一行 lsblk -P 输出，并处理值中的空格和反斜杠。
fn parse_lsblk_line(line: &str) -> Option<StorageDevice> {
    let values = parse_quoted_fields(line);
    let path = values.get("PATH")?.to_string();
    Some(StorageDevice {
        name: values.get("NAME").cloned().unwrap_or_default(),
        path,
        kind: values.get("TYPE").cloned().unwrap_or_default(),
        filesystem: values
            .get("FSTYPE")
            .filter(|value| !value.is_empty())
            .cloned(),
        size_bytes: values
            .get("SIZE")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        readonly: values.get("RO").is_some_and(|value| value == "1"),
        removable: values.get("RM").is_some_and(|value| value == "1"),
        mountpoint: values
            .get("MOUNTPOINT")
            .filter(|value| !value.is_empty())
            .cloned(),
        model: values
            .get("MODEL")
            .filter(|value| !value.is_empty())
            .cloned(),
    })
}

/// 解析 KEY="VALUE" 字段，保证引号内空格不会被误当分隔符。
fn parse_quoted_fields(line: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut cursor = 0;
    let bytes = line.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let key_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'=' {
            cursor += 1;
        }
        if cursor == key_start || cursor >= bytes.len() {
            break;
        }
        let key = &line[key_start..cursor];
        cursor += 1;
        if cursor >= bytes.len() {
            break;
        }
        let quoted = bytes[cursor] == b'"';
        if quoted {
            cursor += 1;
        }
        let mut value = String::new();
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if quoted && byte == b'"' {
                cursor += 1;
                break;
            }
            if !quoted && byte.is_ascii_whitespace() {
                break;
            }
            if byte == b'\\' && cursor + 1 < bytes.len() {
                value.push(bytes[cursor + 1] as char);
                cursor += 2;
            } else {
                value.push(byte as char);
                cursor += 1;
            }
        }
        result.insert(key.to_string(), value);
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
    }
    result
}

/// 解析 df -B1 -P 输出，键为挂载点以便和 findmnt 结果合并。
fn parse_df(input: &str) -> BTreeMap<String, (u64, u64, u64, f64)> {
    input
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 6 {
                return None;
            }
            Some((
                fields[5..].join(" "),
                (
                    fields[1].parse().ok()?,
                    fields[2].parse().ok()?,
                    fields[3].parse().ok()?,
                    fields[4].trim_end_matches('%').parse().ok()?,
                ),
            ))
        })
        .collect()
}

/// 解析 findmnt/proc_mounts 四列，并补齐对应 df 的容量统计。
fn parse_mounts(input: &str, df: &BTreeMap<String, (u64, u64, u64, f64)>) -> Vec<StorageMount> {
    input
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 4 || !fields[0].starts_with('/') {
                return None;
            }
            let (total_bytes, used_bytes, available_bytes, usage_percent) =
                df.get(fields[0]).copied().unwrap_or_default();
            Some(StorageMount {
                mountpoint: fields[0].to_string(),
                source: fields[1].to_string(),
                filesystem: fields[2].to_string(),
                options: fields[3].to_string(),
                total_bytes,
                used_bytes,
                available_bytes,
                usage_percent,
            })
        })
        .collect()
}

/// 解析 fstab marker，限制字段长度并拒绝未知格式行。
fn parse_fstab(input: &str) -> Vec<FstabEntry> {
    input
        .lines()
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.first().copied() != Some("__FSTAB_ENTRY__") || fields.len() < 8 {
                return None;
            }
            let line_number = fields[1].parse().ok()?;
            if fields[2..].iter().any(|field| field.len() > 512) {
                return None;
            }
            Some(FstabEntry {
                line_number,
                source: fields[2].into(),
                mountpoint: fields[3].into(),
                filesystem: fields[4].into(),
                options: fields[5].into(),
                dump: fields[6].into(),
                pass: fields[7].into(),
            })
        })
        .collect()
}

/// 将 marker 区段拆分为名称到正文的映射。
fn split_sections(input: &str) -> BTreeMap<String, String> {
    let mut sections = BTreeMap::new();
    let mut current = String::new();
    for line in input.lines() {
        if line.starts_with("__") && line.ends_with("__") {
            current = line.trim_matches('_').to_string();
            sections.entry(current.clone()).or_insert_with(String::new);
        } else if !current.is_empty() {
            sections
                .entry(current.clone())
                .or_insert_with(String::new)
                .push_str(&format!("{line}\n"));
        }
    }
    sections
}

/// 根据用户选择构造固定的挂载/fstab shell 命令，并返回预期状态摘要。
fn build_action_command(input: &StorageActionInput) -> AppResult<(String, bool, Option<bool>)> {
    let mountpoint = shell_escape(&input.mountpoint);
    let source = input.source.as_deref().map(shell_escape);
    let filesystem = input.filesystem.as_deref().map(shell_escape);
    let options = input.options.as_deref().unwrap_or("defaults");
    let escaped_options = shell_escape(options);
    let command = match input.action.as_str() {
        "mount" => {
            let source = source.ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "storage",
                    "挂载操作必须指定设备或 UUID",
                )
            })?;
            let mount = if let Some(filesystem) = filesystem {
                format!("mount -t {filesystem} -o {escaped_options} -- {source} {mountpoint}")
            } else {
                format!("mount -o {escaped_options} -- {source} {mountpoint}")
            };
            (
                format!("set -e; mkdir -p -- {mountpoint}; {mount}; findmnt -rn -T {mountpoint}",),
                false,
                Some(true),
            )
        }
        "unmount" => (
            format!("set -e; umount -- {mountpoint}; ! findmnt -rn -T {mountpoint}"),
            false,
            Some(false),
        ),
        "add_fstab" => {
            let source = source.ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "storage",
                    "fstab 条目必须指定设备或 UUID",
                )
            })?;
            let filesystem = filesystem.ok_or_else(|| {
                AppError::new("VALIDATION_FAILED", "storage", "fstab 条目必须指定文件系统")
            })?;
            let dump = shell_escape(input.dump.as_deref().unwrap_or("0"));
            let pass = shell_escape(input.pass.as_deref().unwrap_or("0"));
            (
                format!(
                    "set -e; file=/etc/fstab; backup=$file.1panel-client-backup-$$; tmp=$file.1panel-client-tmp-$$; cp -a -- $file $backup; if awk -v wanted={mountpoint} '($2 == wanted) {{ found=1 }} END {{ exit found ? 0 : 1 }}' $file; then cp -a -- $backup $file; rm -f -- $backup; printf '%s\\n' 'fstab mountpoint already exists' >&2; exit 42; fi; printf '%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n' {source} {mountpoint} {filesystem} {escaped_options} {dump} {pass} >> $tmp; cat $tmp >> $file; rm -f -- $tmp; if ! mount -a -f 2>/dev/null; then cp -a -- $backup $file; rm -f -- $backup; exit 43; fi; rm -f -- $backup",
                ),
                true,
                None,
            )
        }
        "remove_fstab" => {
            let source = source.ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "storage",
                    "删除 fstab 条目必须指定设备或 UUID",
                )
            })?;
            (
                format!(
                    "set -e; file=/etc/fstab; backup=$file.1panel-client-backup-$$; tmp=$file.1panel-client-tmp-$$; cp -a -- $file $backup; awk -v wanted_source={source} -v wanted_mount={mountpoint} 'BEGIN {{ removed=0 }} /^[[:space:]]*#/ {{ print; next }} NF >= 2 && $1 == wanted_source && $2 == wanted_mount {{ removed=1; next }} {{ print }} END {{ if (!removed) exit 44 }}' $file > $tmp || {{ cp -a -- $backup $file; rm -f -- $tmp $backup; exit 44; }}; cat $tmp > $file; rm -f -- $tmp; if ! mount -a -f 2>/dev/null; then cp -a -- $backup $file; rm -f -- $backup; exit 45; fi; rm -f -- $backup",
                ),
                true,
                None,
            )
        }
        _ => {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "storage",
                "不支持的存储操作",
            ))
        }
    };
    Ok(command)
}

/// 校验存储变更输入，禁止根挂载点卸载和 shell 元字符进入命令模板。
fn validate_action(input: &StorageActionInput) -> AppResult<()> {
    validate_server_id(&input.server_id)?;
    validate_mountpoint(&input.mountpoint)?;
    if matches!(input.action.as_str(), "unmount" | "remove_fstab")
        && is_protected_mountpoint(&input.mountpoint)
    {
        return Err(AppError::new(
            "PROTECTED_MOUNT",
            "storage",
            "系统关键挂载点不能由客户端卸载或移除",
        ));
    }
    if input.action == "mount" || input.action == "add_fstab" || input.action == "remove_fstab" {
        let source = input.source.as_deref().ok_or_else(|| {
            AppError::new(
                "VALIDATION_FAILED",
                "storage",
                "该操作必须指定设备、UUID 或 LABEL",
            )
        })?;
        validate_source(source)?;
    }
    if let Some(filesystem) = input.filesystem.as_deref() {
        validate_filesystem(filesystem)?;
    }
    if let Some(options) = input.options.as_deref() {
        validate_options(options)?;
    }
    for value in [input.dump.as_deref(), input.pass.as_deref()]
        .into_iter()
        .flatten()
    {
        if !value.is_empty() && !value.chars().all(|character| character.is_ascii_digit()) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "storage",
                "fstab dump/pass 字段只能是数字",
            ));
        }
    }
    Ok(())
}

/// 校验服务器 ID，避免把异常值传递给 SSH 连接层。
fn validate_server_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "storage",
            "服务器 ID 无效",
        ));
    }
    Ok(())
}

/// 校验绝对挂载路径，并拒绝路径穿越和控制字符。
fn validate_mountpoint(value: &str) -> AppResult<()> {
    if !value.starts_with('/')
        || value.len() > 512
        || value.contains("..")
        || value.chars().any(|character| character.is_control())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "storage",
            "挂载点必须是安全的绝对路径",
        ));
    }
    Ok(())
}

/// 校验设备路径、UUID= 或 LABEL= 标识，不允许空白和 shell 元字符。
fn validate_source(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 512
        || !(value.starts_with('/') || value.starts_with("UUID=") || value.starts_with("LABEL="))
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'/' | b'_' | b'-' | b'.' | b'=' | b':' | b'@' | b'+')
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "storage",
            "设备或 UUID 标识无效",
        ));
    }
    Ok(())
}

/// 校验文件系统白名单，避免把任意 mount helper 名称拼进远端命令。
fn validate_filesystem(value: &str) -> AppResult<()> {
    const ALLOWED: &[&str] = &[
        "auto", "ext2", "ext3", "ext4", "xfs", "btrfs", "vfat", "exfat", "ntfs", "nfs", "nfs4",
        "cifs", "tmpfs", "swap",
    ];
    if !ALLOWED.contains(&value) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "storage",
            "暂不支持该文件系统类型",
        ));
    }
    Ok(())
}

/// 校验 mount/fstab 选项只包含内核支持的键值字符。
fn validate_options(value: &str) -> AppResult<()> {
    if value.len() > 512
        || value.is_empty()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b',' | b'=' | b'_' | b'-' | b'.' | b':' | b'@' | b'/')
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "storage",
            "挂载选项包含不支持的字符",
        ));
    }
    Ok(())
}

/// 返回禁止客户端卸载或从 fstab 移除的核心系统路径。
fn is_protected_mountpoint(value: &str) -> bool {
    matches!(
        value,
        "/" | "/boot" | "/boot/" | "/usr" | "/usr/" | "/etc" | "/etc/"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_action_command, compute_topology, parse_snapshot, validate_action, FstabEntry,
        StorageActionInput, StorageDevice,
    };
    use crate::domain::ssh::{ConnectOutcome, TrustHostKeyInput};
    use crate::infra::db::ServerRepository;
    use crate::security::{CredentialStore, OsCredentialStore};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::sync::Arc;

    /// 构造存储动作测试输入，覆盖设备、挂载点和 fstab 字段的常见组合。
    fn input(action: &str) -> StorageActionInput {
        StorageActionInput {
            server_id: "server-1".into(),
            action: action.into(),
            source: Some("UUID=abc-123".into()),
            mountpoint: "/mnt/data".into(),
            filesystem: Some("ext4".into()),
            options: Some("defaults,noatime".into()),
            dump: Some("0".into()),
            pass: Some("2".into()),
            confirmed: true,
        }
    }

    #[test]
    fn parses_devices_mounts_and_fstab() {
        let value = parse_snapshot(
            "__LSBLK__\nNAME=\"sda1\" PATH=\"/dev/sda1\" TYPE=\"part\" FSTYPE=\"ext4\" SIZE=\"1000\" RO=\"0\" RM=\"0\" MOUNTPOINT=\"/mnt/data\" MODEL=\"Disk Model\"\n__MOUNTS__\n/mnt/data /dev/sda1 ext4 rw,relatime\n__DF__\nFilesystem 1B-blocks Used Available Use% Mounted on\n/dev/sda1 1000 400 600 40% /mnt/data\n__FSTAB__\n__FSTAB_ENTRY__\t7\tUUID=abc-123\t/mnt/data\text4\tdefaults\t0\t2\n",
        )
        .expect("snapshot");
        assert_eq!(value.devices[0].path, "/dev/sda1");
        assert_eq!(value.mounts[0].available_bytes, 600);
        let entry: &FstabEntry = &value.fstab[0];
        assert_eq!(entry.line_number, 7);
        assert_eq!(entry.filesystem, "ext4");
        assert_eq!(value.topology.partitions, 1);
    }

    /// 验证块设备拓扑能把 disk/part/raid/lvm 分类汇总。
    #[test]
    fn computes_storage_topology() {
        let device = |kind: &str| StorageDevice {
            name: kind.into(),
            path: format!("/dev/{kind}"),
            kind: kind.into(),
            filesystem: None,
            size_bytes: 0,
            readonly: false,
            removable: false,
            mountpoint: None,
            model: None,
        };
        let topology = compute_topology(&[
            device("disk"),
            device("part"),
            device("raid1"),
            device("lvm"),
            device("loop"),
        ]);
        assert_eq!(topology.disks, 1);
        assert_eq!(topology.partitions, 1);
        assert_eq!(topology.raid_arrays, 1);
        assert_eq!(topology.lvm_volumes, 1);
        assert_eq!(topology.other_devices, 1);
    }

    #[test]
    fn validates_protected_and_unsafe_mounts() {
        let mut protected = input("unmount");
        protected.mountpoint = "/".into();
        assert!(validate_action(&protected).is_err());
        let mut unsafe_source = input("mount");
        unsafe_source.source = Some("/dev/sda;touch".into());
        assert!(validate_action(&unsafe_source).is_err());
    }

    #[test]
    fn builds_atomic_fstab_command() {
        let (command, fstab, mounted) = build_action_command(&input("add_fstab")).expect("command");
        assert!(fstab);
        assert!(mounted.is_none());
        assert!(command.contains("mount -a -f"));
        assert!(command.contains("1panel-client-backup"));
    }

    #[test]
    fn builds_mount_and_unmount_commands() {
        let (mount, _, mounted) = build_action_command(&input("mount")).expect("mount");
        assert_eq!(mounted, Some(true));
        assert!(mount.contains("mount -t 'ext4'"));
        let (unmount, _, mounted) = build_action_command(&input("unmount")).expect("unmount");
        assert_eq!(mounted, Some(false));
        assert!(unmount.contains("umount"));
    }

    /// 在显式提供本机数据库和服务器 ID 时，只读验证真实远端存储快照。
    #[tokio::test]
    #[ignore = "需要用户已授权的真实测试节点环境变量"]
    async fn real_storage_snapshot() -> crate::errors::AppResult<()> {
        let db_path = std::env::var("ONEPANEL_CLIENT_DB").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "storage", "缺少本机测试数据库路径")
        })?;
        let server_id = std::env::var("ONEPANEL_CLIENT_SERVER_ID").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "storage", "缺少测试服务器 ID")
        })?;
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect_with(SqliteConnectOptions::new().filename(db_path))
            .await
            .map_err(crate::errors::AppError::database)?;
        let credentials: Arc<dyn CredentialStore> =
            Arc::new(OsCredentialStore::new("com.agentless.servermanager"));
        let servers = ServerRepository::new(pool, credentials);
        let ssh = crate::domain::ssh::SshConnectionManager::new(servers);
        if let ConnectOutcome::HostKey(challenge) = ssh.connect(&server_id).await? {
            ssh.trust(TrustHostKeyInput {
                server_id: challenge.server_id,
                host: challenge.host,
                port: challenge.port,
                key_type: challenge.key_type,
                fingerprint: challenge.fingerprint,
            })
            .await?;
        }
        let value = super::snapshot(&ssh, &server_id).await?;
        assert!(!value.mounts.is_empty(), "真实服务器应返回至少一个挂载点");
        assert!(value.mounts.iter().any(|mount| mount.mountpoint == "/"));
        Ok(())
    }
}
