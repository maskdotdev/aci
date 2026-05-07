import os
from pathlib import Path

class Service:
    def run(self):
        print(os.getcwd())

def main():
    Service().run()

value = main()
