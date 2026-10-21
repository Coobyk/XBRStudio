import os, subprocess, time
WORK = "/tmp/gui_shot"
os.environ["DISPLAY"] = ":99"
xvfb = subprocess.Popen(["Xvfb", ":99", "-screen", "0", "1400x900x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(2)
gui = subprocess.Popen(["/data/workspace/XBRStudio/target/debug/gui", "/tmp/gui_shot/stone.png"],
                       stdout=open(f"{WORK}/gui.log", "w"), stderr=subprocess.STDOUT)
time.sleep(4)

def cpu():
    with open(f"/proc/{gui.pid}/stat") as f:
        parts = f.read().split()
    return (int(parts[13]) + int(parts[14])) / os.sysconf("SC_CLK_TCK")

def rss():
    with open(f"/proc/{gui.pid}/statm") as f:
        return int(f.read().split()[1]) * 4096 / 1024 / 1024

start = cpu()
time.sleep(5)
idle = cpu() - start
print(f"idle 5s: {idle:.2f}s cpu, {rss():.0f} MB rss")

# Click through the toolbar and the slider.
start = cpu()
begin = time.time()
for x in (135, 260, 400, 550, 730, 850, 979, 1040):
    subprocess.run(["xdotool", "mousemove", str(x), "201"], check=False)
    time.sleep(0.05)
    subprocess.run(["xdotool", "click", "1"], check=False)
    time.sleep(0.05)
for x in range(150, 460, 20):
    subprocess.run(["xdotool", "mousemove", str(x), "806"], check=False)
    time.sleep(0.03)
    subprocess.run(["xdotool", "click", "1"], check=False)
    time.sleep(0.03)
elapsed = time.time() - begin
used = cpu() - start
print(f"interactions: {elapsed:.2f}s wall, {used:.2f}s cpu ({used/elapsed*100:.0f}% of one core)")
gui.terminate(); xvfb.terminate()
