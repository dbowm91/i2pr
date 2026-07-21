"""Final-rendered configuration checks for the two pinned reference routers."""

from __future__ import annotations

from dataclasses import dataclass


class ConfigurationContractError(ValueError):
    """A rendered reference configuration is malformed or unsafe."""


@dataclass(frozen=True)
class ParsedIni:
    root: dict[str, str]
    sections: dict[str, dict[str, str]]


def _pairs(lines: list[str], *, separator: str = "=") -> dict[str, str]:
    result: dict[str, str] = {}
    for line_number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith(";"):
            continue
        if separator not in stripped:
            raise ConfigurationContractError(f"configuration line {line_number} is not key=value")
        key, value = (part.strip() for part in stripped.split(separator, 1))
        if not key or key in result:
            raise ConfigurationContractError(f"duplicate or empty configuration key at line {line_number}")
        result[key] = value
    return result


def parse_java_properties(rendered: str) -> dict[str, str]:
    """Parse the restricted Java property template without accepting sections."""

    result = _pairs(rendered.splitlines())
    if any("[" in key or "]" in key for key in result):
        raise ConfigurationContractError("Java configuration unexpectedly contains a section")
    return result


def assert_java_private_configuration(
    rendered: str, *, address: str, port: int, network_id: int, ipv6: bool = False
) -> None:
    values = parse_java_properties(rendered)
    expected = {
        "i2p.dir.config": "@CONFIG_DIR@",
        "i2np.allowLocal": "true",
        "i2np.ntcp.enable": "true",
        "i2np.ntcp.hostname": address,
        "i2np.ntcp.port": str(port),
        "i2np.ntcp.autoip": "false",
        "i2np.ntcp.autoport": "false",
        "i2np.ntcp.ipv6": "true" if ipv6 else "false",
        "i2np.ntcp2.enable": "true",
        "i2np.upnp.enable": "false",
        "router.networkID": str(network_id),
        "router.reseedDisable": "true",
        "router.updateDisabled": "true",
        "router.floodfillParticipant": "false",
        "router.sharePercentage": "0",
        "router.maxParticipatingTunnels": "0",
        "router.newsRefreshFrequency": "0",
        "i2np.udp.enable": "false",
        "i2np.udp.address": "",
        "i2np.udp.port": "0",
        "i2np.udp.autoip": "false",
        "crypto.edh.precalc.min": "1",
        "crypto.edh.precalc.max": "4",
        "crypto.edh.precalc.delay": "999999",
    }
    if values != {**expected, "i2p.dir.config": values.get("i2p.dir.config", "")}:
        raise ConfigurationContractError("Java rendered configuration has unexpected keys or values")
    if values.get("i2p.dir.config", "").startswith("/") is False:
        raise ConfigurationContractError("Java configuration directory is not confined")
    if values.get("router.networkID") != str(network_id):
        raise ConfigurationContractError("Java network ID was not rendered explicitly")


def parse_i2pd_ini(rendered: str) -> ParsedIni:
    root: dict[str, str] = {}
    sections: dict[str, dict[str, str]] = {}
    current = root
    for line_number, line in enumerate(rendered.splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith(";"):
            continue
        if stripped.startswith("[") and stripped.endswith("]"):
            section = stripped[1:-1].strip()
            if not section or section in sections:
                raise ConfigurationContractError(f"duplicate or empty i2pd section at line {line_number}")
            current = sections[section] = {}
            continue
        if "=" not in stripped:
            raise ConfigurationContractError(f"i2pd configuration line {line_number} is not key=value")
        key, value = (part.strip() for part in stripped.split("=", 1))
        if not key or key in current:
            raise ConfigurationContractError(f"duplicate or empty i2pd key at line {line_number}")
        current[key] = value
    return ParsedIni(root, sections)


def assert_i2pd_private_configuration(
    rendered: str, *, address: str, port: int, network_id: int, ipv6: bool = False
) -> None:
    parsed = parse_i2pd_ini(rendered)
    root = parsed.root
    expected_root = {
        "daemon", "netid", "address4", "address6", "host", "port", "ipv4", "ipv6",
        "notransit", "floodfill", "reservedrange",
    }
    if set(root) != expected_root:
        raise ConfigurationContractError("i2pd root configuration keys drifted")
    if root.get("daemon") != "false" or root.get("netid") != str(network_id):
        raise ConfigurationContractError("i2pd foreground/private network contract is missing")
    if root.get("host") != ("" if ipv6 else address) or root.get("port") != str(port):
        raise ConfigurationContractError("i2pd endpoint was not rendered explicitly")
    if root.get("address6") != (address if ipv6 else ""):
        raise ConfigurationContractError("i2pd IPv6 endpoint was not rendered explicitly")
    if root.get("notransit") != "true" or root.get("floodfill") != "false":
        raise ConfigurationContractError("i2pd transit/floodfill safety contract is missing")
    if root.get("reservedrange") != "false":
        raise ConfigurationContractError("i2pd reserved-range check must be disabled for sealed-namespace tests")
    expected_sections = {"ntcp2", "ssu2", "http", "httpproxy", "socksproxy", "sam", "i2cp", "i2pcontrol", "upnp", "reseed"}
    if set(parsed.sections) != expected_sections:
        raise ConfigurationContractError("i2pd service sections drifted")
    if parsed.sections["ntcp2"] != {"enabled": "true", "published": "true", "port": str(port)}:
        raise ConfigurationContractError("i2pd NTCP2 listener contract is not exact")
    for section in expected_sections - {"ntcp2", "reseed"}:
        values = parsed.sections[section]
        if values.get("enabled") != "false":
            raise ConfigurationContractError(f"unexpected enabled i2pd service: {section}")
        if "published" in values and values["published"] != "false":
            raise ConfigurationContractError(f"unexpected published i2pd service: {section}")
    if parsed.sections["reseed"] != {"verify": "true", "urls": "", "threshold": "0"}:
        raise ConfigurationContractError("i2pd reseed configuration is not disabled exactly")
