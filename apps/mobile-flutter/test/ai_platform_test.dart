import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_mobile/ai_platform.dart';
void main(){test('AI contract is local-safe',(){expect(AiPlatform.version,1);expect(AiPlatform.nativeAuthenticated,false);expect(AiPlatform.cloudUrl,'https://openvietgis.cloud/');expect(AiPlatform.actions,contains('cancel'));});}
