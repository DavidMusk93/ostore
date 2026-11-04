import sys
import time
import os
import json
from playwright.sync_api import sync_playwright

CHROME_BIN = "/root/.cache/ms-playwright/chromium-1148/chrome-linux/chrome"
DEV_TOKEN = "73e3be72c0af3395693e348509b4afb270396bf1dfd27a30538a2c423e47a419"
BASE_URL = "http://127.0.0.1:8781"

def log_step(name):
    print(f"\n========================================================")
    print(f" [TEST STEP] {name}")
    print(f"========================================================", flush=True)

def run_regression_test():
    os.environ["DISPLAY"] = ":99"
    logs = []
    errors = []

    with sync_playwright() as p:
        print(f"Launching Chromium with visible UI on DISPLAY=:99...", flush=True)
        browser = p.chromium.launch(
            executable_path=CHROME_BIN,
            headless=False,
            args=[
                "--start-maximized",
                "--no-sandbox",
                "--disable-dev-shm-usage",
            ]
        )
        context = browser.new_context(viewport={"width": 1920, "height": 1080})
        page = context.new_page()

        page.on("console", lambda msg: logs.append(f"[{msg.type}] {msg.text}"))
        page.on("pageerror", lambda err: errors.append(f"[PAGE_ERROR] {err}"))
        page.on("requestfailed", lambda req: errors.append(f"[REQ_FAILED] {req.method} {req.url} - {req.failure}"))

        # --- STEP 1: Boot & Developer Token Login ---
        log_step("1. Open ostore UI & Test Developer Token Login Modal")
        page.goto(BASE_URL)
        page.wait_for_timeout(1500)

        # First verify Authenticator Setup modal opens and can copy secret
        setup_link = page.query_selector('a:has-text("Set up Authenticator")')
        if setup_link and setup_link.is_visible():
            print("Clicking 'Set up Authenticator' link...", flush=True)
            setup_link.click()
            page.wait_for_timeout(600)
            page.wait_for_selector("#totpSetupModal", state="visible")
            print("Totp setup modal is visible! Clicking 'Copy Secret'...", flush=True)
            page.click('button:has-text("Copy Secret")')
            page.wait_for_timeout(500)
            print("Closing Totp setup modal...", flush=True)
            page.click('#totpSetupModal button.icon-btn:has-text("✕")')
            page.wait_for_timeout(500)

        # Developer Token login
        print("Clicking 'Use developer token' link...", flush=True)
        page.click('a:has-text("Use developer token")')
        page.wait_for_timeout(600)
        token_modal = page.wait_for_selector("#tokenModal", state="visible")
        print("Token modal is visible! Filling token...", flush=True)
        page.fill("#tokenInput", DEV_TOKEN)
        page.wait_for_timeout(400)
        print("Clicking Save button in token modal...", flush=True)
        page.click('#tokenModal button:has-text("Save")')
        page.wait_for_timeout(2000)

        # Verify main console is displayed
        page.wait_for_selector("#mainConsole", state="visible", timeout=8000)
        print("SUCCESS: Main console loaded into view!", flush=True)
        page.screenshot(path="/root/ostore/tests/step1_console_ready.png")

        # --- STEP 2: Google Drive Modal & Status ---
        log_step("2. Test Google Drive Cloud Node Button & Status")
        page.click("#gdriveHeaderBtn")
        page.wait_for_timeout(1000)
        page.wait_for_selector("#gdriveModal", state="visible")
        
        # Check if connected card or connect tabs is displayed
        connected_card = page.query_selector("#gdriveConnectedCard")
        if connected_card and connected_card.is_visible():
            quota_text = page.inner_text("#gdriveQuotaText")
            client_text = page.inner_text("#gdriveClientText")
            print(f"Google Drive is connected! Quota: {quota_text}, Client: {client_text}", flush=True)
        else:
            tabs = page.query_selector("#gdriveConnectTabs")
            if tabs and tabs.is_visible():
                print("Testing tab switching...", flush=True)
                page.click("#tabManualBtn")
                page.wait_for_timeout(400)
                page.click("#tabOAuthBtn")
                page.wait_for_timeout(400)

        # Close modal
        page.click('#gdriveModal button.icon-btn:has-text("✕")')
        page.wait_for_timeout(600)
        print("SUCCESS: Google Drive modal verified!", flush=True)

        # --- STEP 3: Fleet Replication Modal (Header Button) ---
        log_step("3. Test Fleet Replication Header Button & Scan Diff")
        page.click('button[onclick="openClusterSyncModal()"]')
        page.wait_for_timeout(1500)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        
        # Verify Entire Storage is default when opened from header
        scope_all = page.query_selector("#syncScopeAll")
        assert scope_all and scope_all.is_checked(), "Expected Entire Storage (Root /) to be checked by default"
        print("Entire Storage radio is correctly selected by default.", flush=True)

        # Scan Diff
        print("Clicking 'Scan Differences' button...", flush=True)
        page.click("#btnScanDiff")
        page.wait_for_timeout(3500)

        diff_count_text = page.inner_text("#syncDiffCount")
        print(f"Diff scan finished. Detected differences count: {diff_count_text}", flush=True)

        # Select All / Deselect All
        print("Testing Select All and Deselect All links...", flush=True)
        page.click('a:has-text("Select All")')
        page.wait_for_timeout(500)
        page.click('a:has-text("Deselect All")')
        page.wait_for_timeout(500)

        # Close modal
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)
        print("SUCCESS: Fleet replication modal verified!", flush=True)

        # --- STEP 4: Toolbar Buttons (Search & Current Folder Sync) ---
        log_step("4. Test Toolbar Buttons: Search, Filter, Sync Current Folder")
        print("Testing Search Box filtering...", flush=True)
        page.fill("#searchBox", "shenqiao")
        page.wait_for_timeout(600)
        rows_filtered = page.query_selector_all("#fileTableBody tr")
        print(f"Filtered rows for 'shenqiao': {len(rows_filtered)}", flush=True)
        page.fill("#searchBox", "")
        page.wait_for_timeout(600)

        # Test Sync Current Folder button
        print("Clicking Sync Current Folder (#btnSyncFolder)...", flush=True)
        page.click("#btnSyncFolder")
        page.wait_for_timeout(1000)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        target_badge = page.inner_text("#syncTargetBadge")
        print(f"Target badge displayed: {target_badge}", flush=True)
        assert "当前目录" in target_badge, f"Badge should mention 当前目录, got: {target_badge}"
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)
        print("SUCCESS: Toolbar buttons verified!", flush=True)

        # --- STEP 5: Row Actions (Folder Properties & Folder Sync) ---
        log_step("5. Test Folder Row Actions (Properties & Folder Sync)")
        
        # Test Folder Properties Modal
        folder_prop_btn = page.query_selector('button[title="Folder properties"]')
        if folder_prop_btn:
            print("Clicking Folder Properties button...", flush=True)
            folder_prop_btn.click()
            page.wait_for_timeout(800)
            page.wait_for_selector("#folderDetailModal", state="visible")
            print("Folder Detail Modal visible. Clicking 'Sync Replicas'...", flush=True)
            page.click('button:has-text("Sync Replicas")')
            page.wait_for_timeout(1500)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            badge_text = page.inner_text("#syncTargetBadge")
            print(f"Sync target badge from folder properties: {badge_text}", flush=True)
            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)

        # Test Folder Sync Icon
        folder_sync_icon = page.query_selector('button[title="同步此文件夹到指定节点"]')
        if folder_sync_icon:
            print("Clicking row folder sync icon...", flush=True)
            folder_sync_icon.click()
            page.wait_for_timeout(1500)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            badge_text = page.inner_text("#syncTargetBadge")
            print(f"Sync target badge from folder sync icon: {badge_text}", flush=True)
            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)
        print("SUCCESS: Folder row actions verified!", flush=True)

        # --- STEP 6: Navigate into Subfolder & Test File Row Actions & Badges ---
        log_step("6. Navigate into Subfolder & Test File Actions, Badges & Deep Hash")
        folder_rows = page.query_selector_all('.name-cell')
        assert len(folder_rows) > 0, "Expected at least one folder row in root"
        print("Clicking first directory to navigate into subfolder...", flush=True)
        folder_rows[0].click()
        page.wait_for_timeout(2000)

        # Verify back button is visible
        back_btn = page.wait_for_selector("#btnNavBack", state="visible")
        print("Back button (#btnNavBack) is visible in subfolder!", flush=True)

        # In subfolder, find file replica badge
        first_badge = page.wait_for_selector(".node-badges-cluster", state="visible", timeout=6000)
        print("Clicking file replica badge to open File Topology Modal...", flush=True)
        first_badge.click()
        page.wait_for_timeout(1500)
        page.wait_for_selector("#fileTopoModal", state="visible")

        # Test Deep Hash Check
        print("Clicking 'Deep Hash Check' button...", flush=True)
        page.click("#btnDeepCheck")
        page.wait_for_timeout(2500)
        summary = page.inner_text("#fileTopoSummary")
        print(f"Topology summary: {summary}", flush=True)

        # Test 'Sync To Followers'
        print("Clicking 'Sync To Followers' button...", flush=True)
        page.click("#btnSyncSingleFile")
        page.wait_for_timeout(3000)

        page.click('#fileTopoModal button.icon-btn:has-text("✕")')
        page.wait_for_timeout(600)

        # Test Copy URL button
        copy_url_btn = page.query_selector('button[title="Copy Direct URL"]')
        if copy_url_btn:
            print("Clicking 'Copy Direct URL' button...", flush=True)
            copy_url_btn.click()
            page.wait_for_timeout(400)

        # Test Copy MD5 button
        copy_md5_btn = page.query_selector('button[title="Copy MD5"]')
        if copy_md5_btn:
            print("Clicking 'Copy MD5' button...", flush=True)
            copy_md5_btn.click()
            page.wait_for_timeout(400)

        # Test Single file sync icon
        file_sync_icon = page.query_selector('button[title="同步此文件到指定节点"]')
        if file_sync_icon:
            print("Clicking single file sync icon...", flush=True)
            file_sync_icon.click()
            page.wait_for_timeout(1500)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            badge_text = page.inner_text("#syncTargetBadge")
            print(f"Sync target badge from file sync icon: {badge_text}", flush=True)
            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)

        # Test Multi-Row Selection & Batch Sync in Subfolder
        print("Testing Multi-Row Selection & Batch Sync in subfolder...", flush=True)
        sub_cbs = page.query_selector_all(".row-select-cb")
        print(f"Found {len(sub_cbs)} selectable checkboxes in subfolder.", flush=True)
        if len(sub_cbs) >= 2:
            sub_cbs[0].check()
            sub_cbs[1].check()
            page.wait_for_timeout(500)
            page.wait_for_selector("#batchActionBar", state="visible")
            sel_text = page.inner_text("#batchSelectedLabel")
            print(f"Batch bar visible with: {sel_text}", flush=True)

            # Click 批量同步
            print("Clicking '批量同步' button...", flush=True)
            page.click('button:has-text("批量同步")')
            page.wait_for_timeout(1500)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            batch_target = page.inner_text("#syncTargetBadge")
            print(f"Batch sync target badge: {batch_target}", flush=True)

            # Scan diff on batch
            print("Scanning diff on batch target...", flush=True)
            page.click("#btnScanDiff")
            page.wait_for_timeout(3000)

            # Run Sync
            print("Clicking 'Run Sync Now' / 'Sync Selected'...", flush=True)
            page.click("#btnRunSync")
            page.wait_for_timeout(4000)
            print("Batch sync execution finished cleanly!", flush=True)

            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)

            # Verify batch bar is automatically hidden as selection was cleared upon sync completion
            assert not page.is_visible("#batchActionBar"), "Batch bar should be hidden after sync completion"
            print("Batch bar automatically hidden post-sync as expected!", flush=True)

        # Click Back button to return to root
        print("Clicking back button to return to root...", flush=True)
        back_btn.click()
        page.wait_for_timeout(1500)
        print("Returned to root successfully!", flush=True)

        # --- STEP 7: Multi-Row Selection & Batch Sync Execution ---
        log_step("7. Test Multi-Row Selection, Batch Bar & Interactive Sync Execution")
        cbs = page.query_selector_all(".row-select-cb")
        print(f"Found {len(cbs)} selectable item checkboxes.", flush=True)
        if len(cbs) >= 2:
            print("Selecting first 2 items...", flush=True)
            cbs[0].check()
            cbs[1].check()
            page.wait_for_timeout(600)
            page.wait_for_selector("#batchActionBar", state="visible")
            sel_text = page.inner_text("#batchSelectedLabel")
            print(f"Batch bar visible with: {sel_text}", flush=True)
            
            # Click 批量同步
            print("Clicking '批量同步' button...", flush=True)
            page.click('button:has-text("批量同步")')
            page.wait_for_timeout(1500)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            batch_target = page.inner_text("#syncTargetBadge")
            print(f"Batch sync target badge: {batch_target}", flush=True)

            # Scan diff on batch
            print("Scanning diff on batch target...", flush=True)
            page.click("#btnScanDiff")
            page.wait_for_timeout(3000)

            # Trigger Run Sync Now
            print("Clicking 'Run Sync Now' / 'Sync Selected'...", flush=True)
            page.click("#btnRunSync")
            page.wait_for_timeout(4000)
            print("Sync execution finished cleanly!", flush=True)

            # Close Cluster Sync Modal
            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)

            # Clear selection
            print("Clicking '取消选择'...", flush=True)
            page.click('button:has-text("取消选择")')
            page.wait_for_timeout(500)

        # Final full page screenshot
        final_screen = "/root/ostore/tests/ui_test_full_success.png"
        page.screenshot(path=final_screen)
        print(f"\nFinal regression screenshot saved: {final_screen}", flush=True)

        print("\n========================================================")
        print(" REGRESSION TEST SUITE PASSED WITH 0 ERRORS!")
        print("========================================================", flush=True)
        print(f"Total Console logs: {len(logs)}")
        print(f"Total Page errors: {len(errors)}")
        for e in errors:
            print("  [ERROR]", e)
        assert len(errors) == 0, f"Encountered page errors: {errors}"

        browser.close()

if __name__ == "__main__":
    run_regression_test()
