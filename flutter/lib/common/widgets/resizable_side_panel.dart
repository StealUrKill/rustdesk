import 'package:flutter/material.dart';
import 'dart:math';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:get/get.dart';

const String kOptionAbTagsPanelWidth = 'ab-tags-panel-width';
const String kOptionAccessibleDevicesPanelWidth =
    'accessible-devices-panel-width';

class ResizablePanelController {
  final String optionKey;
  final double minWidth;
  final double maxWidth;
  late final RxDouble width;

  static const double dividerHitWidth = 12.0;
  static const double minContentWidth = 120.0;

  ResizablePanelController({
    required this.optionKey,
    required double defaultWidth,
    this.minWidth = 150,
    this.maxWidth = 300,
  }) {
    final saved = double.tryParse(bind.mainGetLocalOption(key: optionKey));
    width =
        RxDouble((saved ?? defaultWidth).clamp(minWidth, maxWidth).toDouble());
  }

  void _onDrag(double dx) {
    width.value = (width.value + dx).clamp(minWidth, maxWidth).toDouble();
  }

  void _persist() {
    bind.mainSetLocalOption(
        key: optionKey, value: width.value.toStringAsFixed(0));
  }

  double _effectiveWidth(double available) {
    if (!available.isFinite) return width.value;
    final maxAllowed = available - dividerHitWidth - minContentWidth;
    return width.value.clamp(minWidth, max(minWidth, maxAllowed)).toDouble();
  }

  Widget _buildDivider(BuildContext context) {
    final sign = Directionality.of(context) == TextDirection.rtl ? -1.0 : 1.0;
    return MouseRegion(
      cursor: SystemMouseCursors.resizeLeftRight,
      child: GestureDetector(
        behavior: HitTestBehavior.translucent,
        onHorizontalDragUpdate: (details) => _onDrag(sign * details.delta.dx),
        onHorizontalDragEnd: (_) => _persist(),
        onHorizontalDragCancel: _persist,
        child: Container(
          width: dividerHitWidth,
          height: double.infinity,
          color: Colors.transparent,
        ),
      ),
    );
  }

  Widget wrap(BuildContext context, double available, {required Widget child}) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Obx(() => SizedBox(width: _effectiveWidth(available), child: child)),
        _buildDivider(context),
      ],
    );
  }
}
