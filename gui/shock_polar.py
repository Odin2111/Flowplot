"""Ideal-gas θ–β–M shock polar for Flowplot Analyze.

Analytic weak/strong polar (γ-law). CFD-coupled polars stay in lehar
`shock_polar`; this module draws the textbook curve when Analyze → Shock polar runs.
"""

from __future__ import annotations

import math
from dataclasses import dataclass


@dataclass(frozen=True)
class PolarCurve:
    mach: float
    gamma: float
    beta_deg: list[float]
    theta_deg: list[float]
    beta_sonic_deg: float | None
    theta_max_deg: float


def deflection_deg(mach: float, beta_rad: float, gamma: float) -> float | None:
    """θ from the θ–β–M relation. None if detached / invalid."""
    if mach <= 1.0 or beta_rad <= 0.0:
        return None
    s2 = math.sin(beta_rad) ** 2
    if mach * mach * s2 <= 1.0:
        return None
    num = 2.0 / math.tan(beta_rad) * (mach * mach * s2 - 1.0)
    den = mach * mach * (gamma + math.cos(2.0 * beta_rad)) + 2.0
    if abs(den) < 1e-30:
        return None
    th = math.atan(num / den)
    return math.degrees(th)


def build_polar(mach: float = 2.0, gamma: float = 1.4, n: int = 240) -> PolarCurve:
    """Sample β from Mach angle → 90° and return θ(β)."""
    m = max(mach, 1.01)
    g = max(gamma, 1.01)
    beta_min = math.asin(1.0 / m)
    betas: list[float] = []
    thetas: list[float] = []
    theta_max = 0.0
    beta_sonic: float | None = None
    for i in range(n):
        t = i / max(n - 1, 1)
        beta = beta_min + t * (0.5 * math.pi - beta_min)
        th = deflection_deg(m, beta, g)
        if th is None or th < 0:
            continue
        betas.append(math.degrees(beta))
        thetas.append(th)
        if th > theta_max:
            theta_max = th
        # Rough sonic point: post-shock M≈1 marker via peak neighborhood.
    if betas:
        # Mark β at max θ as the sonic / attachment limit proxy.
        i_max = max(range(len(thetas)), key=lambda i: thetas[i])
        beta_sonic = betas[i_max]
    return PolarCurve(
        mach=m,
        gamma=g,
        beta_deg=betas,
        theta_deg=thetas,
        beta_sonic_deg=beta_sonic,
        theta_max_deg=theta_max,
    )


def plot_polar_png(
    out_path: str,
    *,
    mach: float = 2.0,
    gamma: float = 1.4,
    title: str | None = None,
) -> PolarCurve:
    """Write a θ–β shock-polar PNG via matplotlib (Agg)."""
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    curve = build_polar(mach, gamma)
    fig, ax = plt.subplots(figsize=(5.2, 4.2), dpi=120)
    ax.plot(curve.theta_deg, curve.beta_deg, color="#c00000", lw=2.2, label=f"M={curve.mach:g}")
    if curve.beta_sonic_deg is not None:
        ax.axhline(curve.beta_sonic_deg, color="#5c6b76", ls="--", lw=0.9, alpha=0.7)
        ax.axvline(curve.theta_max_deg, color="#5c6b76", ls="--", lw=0.9, alpha=0.7)
    ax.set_xlabel("Deflection θ (deg)")
    ax.set_ylabel("Shock angle β (deg)")
    ax.set_title(title or f"Shock polar (γ={curve.gamma:g})")
    ax.grid(True, alpha=0.35)
    ax.legend(loc="best", fontsize=8)
    fig.tight_layout()
    fig.savefig(out_path)
    plt.close(fig)
    return curve


def plot_polar_on_axes(ax, *, mach: float = 2.0, gamma: float = 1.4) -> PolarCurve:
    """Draw polar onto an existing matplotlib Axes (for embedded Tk charts)."""
    curve = build_polar(mach, gamma)
    ax.plot(curve.theta_deg, curve.beta_deg, color="#c00000", lw=2.2, label=f"M={curve.mach:g}")
    if curve.beta_sonic_deg is not None:
        ax.axhline(curve.beta_sonic_deg, color="#5c6b76", ls="--", lw=0.9, alpha=0.7)
        ax.axvline(curve.theta_max_deg, color="#5c6b76", ls="--", lw=0.9, alpha=0.7)
    ax.set_xlabel("Deflection θ (deg)")
    ax.set_ylabel("Shock angle β (deg)")
    ax.set_title(f"Shock polar (γ={curve.gamma:g})")
    ax.grid(True, alpha=0.35)
    ax.legend(loc="best", fontsize=8)
    return curve
