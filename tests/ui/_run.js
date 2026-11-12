// Loaded last by run.sh: runs ONE scenario (fresh jsc process per scenario) or lists them.
var RUN_ARGV = typeof arguments !== "undefined" ? [...arguments] : [];
(function (argv) {
  const want = argv[0] || "--list";
  if (want === "--list") {
    SCENARIOS.forEach((s, i) => print(`SCENARIO\t${i}\t${s.name}`));
    return;
  }
  // jsc decodes argv as Latin-1, so scenarios are addressed by index.
  const sc = SCENARIOS[Number(want)];
  if (!sc) {
    print(`FAIL\t${want}\tno such scenario`);
    return;
  }
  let err = null;
  try {
    sc.fn();
    if (!sc.opts.allowMissingFixtures && MODE !== "collect" && H.missing.length) {
      err = `missing fixtures (run tests/ui/recorder/record.sh): ${H.missing.slice(0, 5).join(" ; ")}`;
    } else if (!sc.opts.allowUncaught && H.uncaught.length) {
      const e = H.uncaught[0];
      err = `uncaught async error: ${(e && e.stack) || e}`;
    }
  } catch (e) {
    err = e instanceof AssertionError ? e.message : `${e}\n${(e && e.stack) || ""}`;
  }
  if (MODE === "collect" && typeof HARNESS_REQUESTS_OUT === "string") {
    writeFile(HARNESS_REQUESTS_OUT, JSON.stringify(H.requests));
  }
  print(err ? `FAIL\t${sc.name}\t${err}` : `PASS\t${sc.name}`);
})(RUN_ARGV);
