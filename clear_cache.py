import glob
import shutil

base = r'd:\Caipeilan\DiscoASong'
for d in glob.glob(base + '/**/__pycache__', recursive=True):
    shutil.rmtree(d, ignore_errors=True)
print('Done')
