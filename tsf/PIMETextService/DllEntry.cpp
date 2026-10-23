// PYIME 修改:语言配置写死,不再扫描 PIME 的 backends.json/ime.json。
#include "PIMEImeModule.h"
#include "resource.h"


PIME::ImeModule* g_imeModule = NULL;

BOOL APIENTRY DllMain(HMODULE hModule, DWORD  ul_reason_for_call, LPVOID lpReserved) {
	switch (ul_reason_for_call) {
	case DLL_PROCESS_ATTACH:
		::DisableThreadLibraryCalls(hModule); // disable DllMain calls due to new thread creation
		//::MessageBox(0, L"X!", 0, 0);
		g_imeModule = new PIME::ImeModule(hModule);
		break;
	case DLL_PROCESS_DETACH:
		if(g_imeModule) {
			g_imeModule->Release();
			g_imeModule = NULL;
		}
		break;
	}
	return TRUE;
}

STDAPI DllCanUnloadNow(void) {
	return g_imeModule->canUnloadNow();
}

STDAPI DllGetClassObject(REFCLSID rclsid, REFIID riid, void **ppvObj) {
	return g_imeModule->getClassObject(rclsid, riid, ppvObj);
}

STDAPI DllUnregisterServer(void) {
	return g_imeModule->unregisterServer();
}

// PYIME:语言配置 GUID {87977804-6ADE-4B62-8715-4B1C29435A52}
// 用外部链接保证 GUID 字节落在数据段(static const 会被编译器折成立即数,产物检查查不到)
extern const GUID g_profileGuid;
const GUID g_profileGuid =
{ 0x87977804, 0x6ade, 0x4b62, { 0x87, 0x15, 0x4b, 0x1c, 0x29, 0x43, 0x5a, 0x52 } };

STDAPI DllRegisterServer(void) {
	// 名称、locale 与原 installer/input_methods/pyime/ime.json 一致;不带图标(原 icon 为空)
	Ime::LangProfileInfo langProfile = {
		L"PYIME 拼音",
		g_profileGuid,
		L"zh-Hans-CN",
		L"zh-CN",
		L"",
		0
	};
	wchar_t imeName[] = L"PYIME";
	return g_imeModule->registerServer(imeName, &langProfile, 1);
}
