//! Explicit measurements on a disposable guest, never a release certification.
use super::*;

fn visible(console: &mut Console, text: &str) -> Duration {
    let start = Instant::now();
    loop {
        console.pump(Duration::from_millis(2));
        if console.parser.screen().contents().contains(text) {
            return start.elapsed();
        }
        assert!(start.elapsed() < Duration::from_secs(15), "Missing {text}");
    }
}

fn route(console: &mut Console, command: &str, title: &str) -> Duration {
    console.send(command.as_bytes());
    console.send(b"\r");
    let start = Instant::now();
    visible(console, title);
    while console
        .parser
        .screen()
        .contents()
        .contains("Reading system information")
    {
        console.pump(Duration::from_millis(2));
        assert!(start.elapsed() < Duration::from_secs(15));
    }
    start.elapsed()
}

fn record(rows: &mut String, name: &str, mut samples: Vec<Duration>) {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/console-qualification/guest-benchmark");
    let raw = samples
        .iter()
        .enumerate()
        .map(|(index, value)| format!("{index},{}\n", value.as_micros()))
        .collect::<String>();
    fs::write(
        directory.join(format!("{name}-samples.csv")),
        format!("sample,microseconds\n{raw}"),
    )
    .unwrap();
    samples.sort();
    let percentile = |percent: usize| {
        samples[(samples.len() * percent).div_ceil(100).saturating_sub(1)].as_micros()
    };
    rows.push_str(&format!(
        "{name},{},{},{},{}\n",
        samples.len(),
        percentile(50),
        percentile(95),
        samples.last().unwrap().as_micros()
    ));
}

#[test]
#[ignore = "explicit disposable AMD64 KVM benchmark; run sequentially"]
fn baseline_in_disposable_guest() {
    let boot = Instant::now();
    let mut console = disposable_guest("guest-benchmark");
    let mut rows = "metric,samples,p50_us,p95_us,max_us\n".to_owned();
    // Includes package fixture installation and debug-shell startup, not normal login boot.
    record(&mut rows, "fixture_ready", vec![boot.elapsed()]);
    console.submit("useradd -m -s /bin/bash linosbench; uname -a >/root/linos-benchmark-system.txt; df -B1 / >>/root/linos-benchmark-system.txt; cat /proc/meminfo >>/root/linos-benchmark-system.txt; dpkg-query -W -f='${Package} ${Installed-Size}\\n' >>/root/linos-benchmark-system.txt; printf '%s%s\\n' BENCH_ACCOUNT_ READY");
    console.wait("BENCH_ACCOUNT_READY");
    // A separate, bounded observer samples guest-wide memory and the Home process.
    // Its own shell/sleep/awk overhead is present in both idle and loaded samples.
    console.submit("(for n in $(seq 1 90); do p=$(pgrep -u linosbench -x linos | head -n 1); if test -n \"$p\"; then printf 'sample=%s ' \"$n\"; awk '/MemTotal:|MemAvailable:/{printf \"%s%s \",$1,$2}' /proc/meminfo; awk '/VmRSS:|VmHWM:|Threads:/{printf \"%s%s \",$1,$2}' /proc/$p/status; awk '{printf \"utime=%s stime=%s start=%s\\n\",$14,$15,$22}' /proc/$p/stat; fi; sleep 1; done) >/root/linos-benchmark-samples.txt 2>/dev/null & printf '%s%s\\n' OBSERVER_ READY");
    console.wait("OBSERVER_READY");
    console.send(b"stty rows 32 cols 100; runuser -u linosbench -- env TERM=xterm-256color HOME=/home/linosbench USER=linosbench SHELL=/bin/bash /usr/bin/linos\r");
    record(
        &mut rows,
        "home_start",
        vec![visible(&mut console, "Welcome back, linosbench.")],
    );
    console.pump(Duration::from_secs(12));
    console.snapshot("idle-home");

    for loaded in [false, true] {
        if loaded {
            console.submit("timeout 40s sh -c 'while :; do :; done' >/dev/null 2>&1 & linos_bench_load=$!; printf '%s%s\\n' LOAD_ STARTED");
            console.wait("LOAD_STARTED");
            console.wait("Command completed");
        }
        route(&mut console, "/home", "Welcome back");
        let mut typing = Vec::new();
        for _ in 0..30 {
            console.send(b"z");
            typing.push(visible(&mut console, "│ › z"));
            console.send(b"\x15");
            console.pump(Duration::from_millis(30));
        }
        record(
            &mut rows,
            if loaded {
                "typing_loaded"
            } else {
                "typing_idle"
            },
            typing,
        );
        for (command, title, metric) in [
            ("/resources", "CPU activity, whole machine", "resources"),
            ("/hardware", "Type           ", "hardware"),
            ("/services", "Running services", "services"),
        ] {
            let mut samples = Vec::new();
            for _ in 0..10 {
                route(&mut console, "/home", "Welcome back");
                samples.push(route(&mut console, command, title));
            }
            record(
                &mut rows,
                &format!("{metric}_{}", if loaded { "loaded" } else { "idle" }),
                samples,
            );
        }
        let mut commands = Vec::new();
        for index in 0..15 {
            // The marker is absent from the typed command, so echo cannot satisfy it.
            let marker = format!("BENCH_REPLY_{loaded}_{index}");
            console.send(format!("printf '%s%s\\n' BENCH_ REPLY_{loaded}_{index}\r").as_bytes());
            commands.push(visible(&mut console, &marker));
            console.wait("Command completed");
        }
        record(
            &mut rows,
            if loaded {
                "command_loaded"
            } else {
                "command_idle"
            },
            commands,
        );
        if loaded {
            console.submit("kill \"$linos_bench_load\" 2>/dev/null; wait \"$linos_bench_load\" 2>/dev/null; printf '%s%s\\n' LOAD_ STOPPED");
            console.wait("LOAD_STOPPED");
        }
    }
    fs::write(console.directory.join("latency.csv"), rows).unwrap();
    console.submit("/logout");
    console.wait("root@");
    // Qualification of the new status surface is separate from timed samples.
    // The old baseline binary does not implement these checks.
    if std::env::var_os("LINOS_BENCHMARK_VERIFY_STATUS").is_some() {
        console.submit("systemd-run --wait --unit=linos-benchmark-failed /bin/false; systemctl is-failed --quiet linos-benchmark-failed && runuser -u linosbench -- /usr/bin/linos status >/root/linos-benchmark-status.txt && grep -q linos-benchmark-failed /root/linos-benchmark-status.txt && grep -q 'Resource pressure' /root/linos-benchmark-status.txt && printf '%s%s\\n' STATUS_FAILURE_ OBSERVED");
        console.wait("STATUS_FAILURE_OBSERVED");
    }
    console.submit("sync; poweroff");
    let deadline = Instant::now() + Duration::from_secs(20);
    while console.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "Guest did not shut down");
        console.pump(Duration::from_millis(100));
    }
}
