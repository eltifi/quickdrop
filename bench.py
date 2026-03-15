import asyncio
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor

def fetch_health():
    try:
        req = urllib.request.Request("http://127.0.0.1:3000/health")
        with urllib.request.urlopen(req) as response:
            return response.read()
    except Exception as e:
        return None

async def main():
    loop = asyncio.get_event_loop()
    with ThreadPoolExecutor(max_workers=50) as pool:
        start_time = time.time()
        tasks = []
        for _ in range(5000):
            tasks.append(loop.run_in_executor(pool, fetch_health))

        await asyncio.gather(*tasks)

        end_time = time.time()
        print(f"Time for 5000 requests: {end_time - start_time:.2f} seconds")
        print(f"Requests per second: {5000 / (end_time - start_time):.2f}")

if __name__ == "__main__":
    asyncio.run(main())
