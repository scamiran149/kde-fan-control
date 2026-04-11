/*
 * KDE Fan Control — Value Types Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "types.h"

double millidegreesToCelsius(qint64 millidegrees)
{
    return millidegrees / 1000.0;
}

QString formatTemperature(double celsius)
{
    return QStringLiteral("%1 °C").arg(celsius, 0, 'f', 1);
}

QString formatRpm(int rpm)
{
    if (rpm <= 0) {
        return QStringLiteral("No RPM feedback");
    }
    return QStringLiteral("%1 RPM").arg(rpm);
}

QString formatOutputPercent(double percent)
{
    if (percent < 0.0) {
        return QStringLiteral("No control");
    }
    return QStringLiteral("%1%").arg(qRound(percent));
}