from setuptools import setup
import os

here = os.path.abspath(os.path.dirname(__file__))
README = open(os.path.join(here, 'README.rst')).read()
#NEWS = open(os.path.join(here, 'NEWS.txt')).read()


version = '1.1.0c1'

setup(name='cerridwen',
      version=version,
      description='Moon data library and API server',
      long_description=README,
      author='Leslie P. Polzer',
      author_email='leslie.polzer@gmx.net',
      url='',
      license='Apache',
      classifiers=[
        # Get strings from http://pypi.python.org/pypi?%3Aaction=list_classifiers
        "Development Status :: 4 - Beta"
      , "Environment :: Console"
      , "Intended Audience :: Science/Research"
      , "License :: OSI Approved :: MIT License"
      , "Operating System :: OS Independent"
      , "Programming Language :: Python :: 3"
      , "Topic :: Scientific/Engineering :: Astronomy"
      ],
      maintainer='Leslie P. Polzer',
      maintainer_email='leslie.polzer@gmx.net',
      packages=['cerridwen'],
      requires=['pyswisseph', 'numpy'],
      extras_require={'Flask':['flask']},
      entry_points={
          'console_scripts':
              ['cerridwen = cerridwen.cli:main',
               'cerridwen-server = cerridwen.api_server:main [Flask]']
      })

