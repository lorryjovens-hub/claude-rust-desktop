# Security Advisory

## Overview

This document outlines the security vulnerabilities identified in the project dependencies.

## GitHub Security Advisories

The project has the following vulnerabilities reported by GitHub:

1. **1 High severity vulnerability**
2. **2 Moderate severity vulnerabilities**
3. **1 Low severity vulnerability**

## Vulnerability Details

### Rust Dependencies (Tauri Framework)

The following vulnerabilities are in Tauri framework dependencies and cannot be directly fixed by this project:

#### RUSTSEC-2024-0413 - gtk-rs GTK3 bindings no longer maintained
- **Severity**: Medium
- **Package**: gtk 0.18.2 (transitive)
- **Status**: No fix available - requires Tauri framework update

#### RUSTSEC-2026-0097 - Rand is unsound with a custom logger
- **Severity**: Medium
- **Package**: rand 0.7.3 (transitive via phf_generator)
- **Status**: This is in dev dependencies only (phf_generator used for code generation)
- **Impact**: Only affects development builds, not production

### Node.js Dependencies

The following packages have newer versions available but cannot be updated due to breaking changes:

| Package | Current | Latest | Reason |
|---------|---------|--------|--------|
| lucide-react | 0.563.0 | 1.9.0 | Icon component API changes |
| react-router-dom | 6.22.3 | 7.14.2 | Breaking changes in v7 |
| react-markdown | 9.0.1 | 10.1.0 | Breaking changes in v10 |
| vite | 6.4.2 | 8.0.10 | Build system compatibility |

## Recommended Actions

1. **For Rust vulnerabilities**: Monitor Tauri releases for updates that address these vulnerabilities
2. **For Node.js dependencies**: Plan a migration to newer major versions in a future breaking-change release

## Reporting Security Issues

If you discover a security vulnerability in this project, please report it via:
- GitHub Security Advisories
- Or contact the maintainer directly

## Last Updated

2026-04-24
